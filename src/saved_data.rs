use crate::error::CalculatorDatabaseInconsistencyError;
use crate::variable::Variable;
use num::{bigint::BigInt, rational::BigRational};
use rusqlite::{self, named_params, OptionalExtension, Transaction};
use std::{env, fs::create_dir, io, path::Path};

const DATA_ROOT_DIR_ENV_VAR_NAME: &str = "_B_UTIL_DATA_DIR";
const DATA_DIR_NAME: &str = "bcalc";
const HISTORY_DB_NAME: &str = "saved_data.sqlite";

const CURRENT_DB_VERSION: i64 = 1;
const MINIUM_COMPATIBLE_DB_VERSION: i64 = 1;

const DEFAULT_MAX_HISTORY_SIZE: usize = 100;

const VARIABLE_STORAGE_RADIX: u32 = 10;

#[repr(i64)]
enum MetaInt {
    // The current version of the database schema.
    Version = 1,
    // If `CURRENT_DB_VERSION` is less than `MinimumVersion`, breaking changes have been made to
    // the schema, and we should not attempt to use the database without updating.
    MinimumVersion = 2,
    // The maximum size of the input history before we further items are evicted.
    MaxHistorySize = 3,
}

#[repr(i64)]
enum InputHistoryTag {
    // The front of the input history list. This is the most recent item added to the input history.
    Front = 1,
    // The back of the input history list. This is the item that was added to the input history
    // longest ago and the one that will next be evicted once the history's size has exceeded
    // `MAX_HISTORY_SIZE`.
    Back = 2,
}

/// We will store/load several types of data to/from the file system using SQLite. Some of it is not
/// super conducive to being stored in table format, so our data structures may be a little awkward.
///
/// Several of the tables are basically key/value stores. In these instances, the key values will
/// defined via enums (above). These keys will be stored via aliases to the table's `rowid`, which
/// is documented here: https://www.sqlite.org/lang_createtable.html#rowid
/// As documented, `rowid`s are 64-bit signed integers.
///
/// # Table `meta_int`
/// This table contains key/value metadata where the value is an integer. The possible keys are
/// enumerated and documented by `MetaInt`.
///
/// # Table `input_history`
/// This table effectively stores an ordered list of calculator inputs in order to allow the
/// calculator to retain scrollback across invocations. This is basically implemented as a doubly
/// linked list, with each row storing the `id` of the next row in the list in its `next` column and
/// the `id` of the previous row in the list in its `prev` column.
/// We will manually enforce a limit for the number of rows in this table. When we insert a row, we
/// will check to see if we exceeded that size and, if we did, we will evict the oldest rows from
/// the list until we are within the limit.
///
/// ## Columns
/// ### `id`
/// This will be a `rowid` alias that we will use to point to rows in this table from various other
/// places.
///
/// ### `input`
/// The calculator input.
///
/// ### `next`
/// An `id` within this same table indicating the next row in the list (i.e. the input that was
/// inserted just after this one). May be `NULL` if this is the first item in the list.
///
/// ### `prev`
/// An `id` within this same table indicating the previous row in the list (i.e. the input that was
/// inserted just before this one). May be `NULL` if this is the last item in the list.
///
/// # Table `input_history_tags`
/// This table contains key/value data mapping "tags" to row `id`s in `input_history`. The possible
/// keys are enumerated and documented by `InputHistoryTag`.
///
/// # Table `variable_history`
/// This will store variables and their values so that they can be used again in the future. We will
/// keep track of what row in the `input_history` table last used each variable. When a row is
/// eventually evicted from `input_history`, any variables last used by that row to use will be
/// removed via a `ON DELETE CASCADE` clause on the `last_used_by` column.
///
/// ## Columns
/// ### `name`
/// The name of the variable. This column is defined with `PRIMARY KEY ON CONFLICT REPLACE`, so we
/// can always insert variables without having to worry about whether they already exist.
///
/// ### `numer`
/// The numerator of the value stored by the variable. Although storing an integer would be more
/// efficient, we will instead store this as text because it imposes virtually no limit on how
/// large/precise of a number we can store. It would be more space efficient to use a blob type and
/// store the numbers as binary data, but it's hard to imagine storing enough variables in the
/// database that size really becomes an issue. And it's somewhat convenient for debugging purposes
/// to have the values in the database be human readable.
///
/// ### `denom`
/// The denominator of the value stored by the variable. This is stored as text for the same reason
/// that `numer` is (see above).
///
/// ### `last_used_by`
/// When the variable is set or used, the `id` of the corresponding entry in `input_history` will be
/// stored here. This column will be defined with `ON DELETE CASCADE` so that when the row that it
/// references is evicted from `input_history`, the corresponding rows in this table will also be
/// removed.
pub struct SavedData {
    connection: rusqlite::Connection,
    // This will hold the next `id` in the `input_history` table that we should retrieve when
    // `get_prev_input_history` is called. If it holds `None`, there is no history to load.
    input_history_position: Option<i64>,
}

impl SavedData {
    /// Attempt to open a connection to the database. Our ability to do this depends on our ability
    /// to pull components of the path to the database out of the environment. But we don't want the
    /// whole calculator to completely fail just because an environment variable isn't set. So in
    /// that case, we will return `Ok(None)` instead of an error.
    /// When the database is opened, we remember the index of the input history that is currently
    /// at the front of the history list (the most recent item inserted). This allows us to iterate
    /// through the history without getting the items that we inserted during our session.
    pub fn open() -> Result<Option<SavedData>, Box<dyn std::error::Error>> {
        let data_dir_path_str = match env::var(DATA_ROOT_DIR_ENV_VAR_NAME) {
            Ok(s) => s,
            Err(env::VarError::NotPresent) => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        let data_dir_path = Path::new(&data_dir_path_str).join(DATA_DIR_NAME);
        if let Err(e) = create_dir(data_dir_path.clone()) {
            if e.kind() != io::ErrorKind::AlreadyExists {
                return Err(e.into());
            }
        }
        let db_path = data_dir_path.join(HISTORY_DB_NAME);
        let mut connection = rusqlite::Connection::open(db_path)?;
        connection.execute("PRAGMA foreign_keys = ON;", ())?;

        let transaction = connection.transaction()?;

        transaction.execute(
            "CREATE TABLE IF NOT EXISTS meta_int(
                key INTEGER PRIMARY KEY ASC,
                value INTEGER NOT NULL
            );",
            (),
        )?;
        transaction.execute(
            "INSERT OR IGNORE INTO meta_int (key, value) VALUES (:key, :value)",
            named_params! {
                ":key": MetaInt::MinimumVersion as i64,
                ":value": MINIUM_COMPATIBLE_DB_VERSION,
            },
        )?;
        let minimum_version: i64 = transaction.query_row(
            "SELECT value FROM meta_int WHERE key=:key",
            named_params! {
                ":key": MetaInt::MinimumVersion as i64,
            },
            |row| row.get(0),
        )?;
        if minimum_version > CURRENT_DB_VERSION {
            return Err(CalculatorDatabaseInconsistencyError::new(
                "Database version is not compatible with executable version",
            )
            .into());
        }
        transaction.execute(
            "INSERT OR IGNORE INTO meta_int (key, value) VALUES (:key, :value)",
            named_params! {
                ":key": MetaInt::Version as i64,
                ":value": CURRENT_DB_VERSION,
            },
        )?;
        transaction.execute(
            "INSERT OR IGNORE INTO meta_int (key, value) VALUES (:key, :value)",
            named_params! {
                ":key": MetaInt::MaxHistorySize as i64,
                ":value": DEFAULT_MAX_HISTORY_SIZE,
            },
        )?;

        transaction.execute(
            "CREATE TABLE IF NOT EXISTS input_history(
                id INTEGER PRIMARY KEY ASC,
                input TEXT NOT NULL,
                next REFERENCES input_history(id),
                prev REFERENCES input_history(id)
            );",
            (),
        )?;

        transaction.execute(
            "CREATE TABLE IF NOT EXISTS input_history_tags(
                key INTEGER PRIMARY KEY ASC,
                value REFERENCES input_history(id)
            );",
            (),
        )?;
        transaction.execute(
            "INSERT OR IGNORE INTO input_history_tags (key, value) VALUES (:key, NULL)",
            named_params! {
                ":key": InputHistoryTag::Front as i64,
            },
        )?;
        transaction.execute(
            "INSERT OR IGNORE INTO input_history_tags (key, value) VALUES (:key, NULL)",
            named_params! {
                ":key": InputHistoryTag::Back as i64,
            },
        )?;
        let initial_front: Option<i64> = transaction.query_row(
            "SELECT value FROM input_history_tags WHERE key=:key",
            named_params! {
                ":key": InputHistoryTag::Front as i64,
            },
            |row| row.get(0),
        )?;

        transaction.execute(
            "CREATE TABLE IF NOT EXISTS variable_history(
                name TEXT PRIMARY KEY ON CONFLICT REPLACE,
                numer TEXT NOT NULL,
                denom TEXT NOT NULL,
                last_used_by NOT NULL REFERENCES input_history(id) ON DELETE CASCADE
            );",
            (),
        )?;

        transaction.commit()?;

        Ok(Some(SavedData {
            connection,
            input_history_position: initial_front,
        }))
    }

    /// Adds the given input to the front of the input history list and updates metadata to maintain
    /// the internal mechanisms of the list.
    /// If this causes the history to exceed `MAX_HISTORY_SIZE`, items will be evicted from the
    /// history until the expected maximum size is reached.
    /// Returns the id of the history entry that was inserted.
    pub fn add_to_input_history(&mut self, input: &str) -> Result<i64, Box<dyn std::error::Error>> {
        let mut transaction = self.connection.transaction()?;
        let maybe_orig_front: Option<i64> = transaction.query_row(
            "SELECT value FROM input_history_tags WHERE key=:key",
            named_params! {
                ":key": InputHistoryTag::Front as i64,
            },
            |row| row.get(0),
        )?;

        // Insert the new row
        transaction.execute(
            "INSERT INTO input_history (input, next, prev) VALUES (:input, NULL, :prev)",
            named_params! {
                ":input": input,
                ":prev": maybe_orig_front,
            },
        )?;
        let added_input_id: i64 = transaction.last_insert_rowid();
        // Update the front tag to point to the new front.
        transaction.execute(
            "UPDATE input_history_tags SET value=:tag_value WHERE key=:key",
            named_params! {
                ":key": InputHistoryTag::Front as i64,
                ":tag_value": added_input_id,
            },
        )?;

        match maybe_orig_front {
            Some(orig_front) => {
                // Update the old front to point to the new front.
                transaction.execute(
                    "UPDATE input_history SET next=:new_front WHERE id=:orig_front",
                    named_params! {
                        ":orig_front": orig_front,
                        ":new_front": added_input_id,
                    },
                )?;
            }
            None => {
                // The list was previously empty. We also need to update the back to point to the
                // front.
                transaction.execute(
                    "UPDATE input_history_tags SET value=:tag_value WHERE key=:key",
                    named_params! {
                        ":key": InputHistoryTag::Back as i64,
                        ":tag_value": added_input_id,
                    },
                )?;
            }
        }

        SavedData::enforce_history_size_with_transaction(&mut transaction)?;

        transaction.commit()?;

        Ok(added_input_id)
    }

    fn enforce_history_size_with_transaction(
        transaction: &mut Transaction,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let max_history_size: i64 = SavedData::get_max_history_size_with_transaction(transaction)?;

        if validate_max_history_size(max_history_size).is_err() {
            return Err(CalculatorDatabaseInconsistencyError::new(
                "Stored maximum history size is not valid",
            )
            .into());
        }

        loop {
            let history_size: i64 =
                transaction
                    .query_row("SELECT COUNT(*) FROM input_history", (), |row| row.get(0))?;
            if history_size <= max_history_size {
                break;
            }
            let old_back: i64 = transaction.query_row(
                "SELECT value FROM input_history_tags WHERE key=:key",
                named_params! {
                    ":key": InputHistoryTag::Back as i64,
                },
                |row| row.get(0),
            )?;
            let new_back: i64 = transaction.query_row(
                "SELECT next FROM input_history WHERE id=:id",
                named_params! {
                    ":id": old_back,
                },
                |row| row.get(0),
            )?;
            transaction.execute(
                "UPDATE input_history SET prev=NULL WHERE id=:id",
                named_params! {
                    ":id": new_back,
                },
            )?;
            transaction.execute(
                "UPDATE input_history_tags SET value=:tag_value WHERE key=:key",
                named_params! {
                    ":key": InputHistoryTag::Back as i64,
                    ":tag_value": new_back,
                },
            )?;
            transaction.execute(
                "DELETE FROM input_history WHERE id=:id",
                named_params! {
                    ":id": old_back,
                },
            )?;
        }

        Ok(())
    }

    /// The first time this function is called, it retrieves the history item that was at the front
    /// of the list when `SavedData::open` was called. Each subsequent time, it retrieves the
    /// history item before the one that was retrieved last time, until the earliest history item
    /// is reached, and `Ok(None)` is returned instead.
    pub fn get_prev_input_history(&mut self) -> Result<Option<String>, Box<dyn std::error::Error>> {
        let next_id = match self.input_history_position.clone() {
            Some(i) => i,
            None => return Ok(None),
        };
        // Remember to account for the possibility that we evicted this id from the history already.
        let result: Option<(String, Option<i64>)> = self
            .connection
            .query_row(
                "SELECT input, prev FROM input_history WHERE id=:id",
                named_params! {
                    ":id": next_id,
                },
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;

        match result {
            None => {
                self.input_history_position = None;
                Ok(None)
            }
            Some((input, maybe_prev)) => {
                self.input_history_position = maybe_prev;
                Ok(Some(input))
            }
        }
    }

    /// Sets or updates the variable in the variable history.
    pub fn set_variable(
        &mut self,
        var: &Variable,
        last_used_by_id: i64,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.connection.execute(
            "INSERT INTO variable_history (name, numer, denom, last_used_by)
                    VALUES (:name, :numer, :denom, :last_used_by)",
            named_params! {
                ":name": var.name,
                ":numer": var.value.numer().to_str_radix(VARIABLE_STORAGE_RADIX),
                ":denom": var.value.denom().to_str_radix(VARIABLE_STORAGE_RADIX),
                ":last_used_by": last_used_by_id,
            },
        )?;
        Ok(())
    }

    /// Updates the `last_used_by` field of the variable specified.
    pub fn touch_variable(
        &mut self,
        name: &str,
        last_used_by_id: i64,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.connection.execute(
            "UPDATE variable_history SET last_used_by=:last_used_by WHERE name=:name",
            named_params! {
                ":last_used_by": last_used_by_id,
                ":name": name,
            },
        )?;
        Ok(())
    }

    /// Gets a variable from the variable history and returns it, if it exists.
    pub fn get_variable(
        &mut self,
        name: String,
    ) -> Result<Option<Variable>, Box<dyn std::error::Error>> {
        let result: Option<(String, String)> = self
            .connection
            .query_row(
                "SELECT numer, denom FROM variable_history WHERE name=:name",
                named_params! {
                    ":name": &name,
                },
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;

        let (numer_str, denom_str) = match result {
            None => return Ok(None),
            Some((numer_str, denom_str)) => (numer_str, denom_str),
        };

        let numer = match BigInt::parse_bytes(numer_str.as_bytes(), VARIABLE_STORAGE_RADIX) {
            Some(n) => n,
            None => {
                return Err(CalculatorDatabaseInconsistencyError::new(format!(
                    "Stored numerator ({}) for variable '{}' cannot be parsed",
                    &numer_str, &name
                ))
                .into());
            }
        };
        let denom = match BigInt::parse_bytes(denom_str.as_bytes(), VARIABLE_STORAGE_RADIX) {
            Some(n) => n,
            None => {
                return Err(CalculatorDatabaseInconsistencyError::new(format!(
                    "Stored denominator ({}) for variable '{}' cannot be parsed",
                    &denom_str, &name
                ))
                .into());
            }
        };
        let value = BigRational::new(numer, denom);

        Ok(Some(Variable { name, value }))
    }

    pub fn clear_variable(&mut self, name: &str) -> Result<(), Box<dyn std::error::Error>> {
        self.connection.execute(
            "DELETE FROM variable_history WHERE name=:name",
            named_params! {
                ":name": name,
            },
        )?;
        Ok(())
    }

    fn get_max_history_size_with_transaction(
        transaction: &mut Transaction,
    ) -> Result<i64, Box<dyn std::error::Error>> {
        let size = transaction.query_row(
            "SELECT value FROM meta_int WHERE key=:key",
            named_params! {
                ":key": MetaInt::MaxHistorySize as i64,
            },
            |row| row.get(0),
        )?;

        Ok(size)
    }

    pub fn get_max_history_size(&mut self) -> Result<i64, Box<dyn std::error::Error>> {
        let mut transaction = self.connection.transaction()?;
        let size = SavedData::get_max_history_size_with_transaction(&mut transaction)?;
        transaction.commit()?;
        Ok(size)
    }

    /// If the size passed is provided by the user, the caller probably ought to validate it via
    /// `validate_max_history_size` in advance because this function is less forgiving and will
    /// return a `CalculatorDatabaseInconsistencyError` if the size is not valid.
    pub fn set_max_history_size(&mut self, size: i64) -> Result<(), Box<dyn std::error::Error>> {
        if validate_max_history_size(size).is_err() {
            return Err(CalculatorDatabaseInconsistencyError::new(
                "Attempted to set a maximum history size that is not valid",
            )
            .into());
        }

        let mut transaction = self.connection.transaction()?;
        transaction.execute(
            "INSERT OR REPLACE INTO meta_int (key, value) VALUES (:key, :value)",
            named_params! {
                ":key": MetaInt::MaxHistorySize as i64,
                ":value": size,
            },
        )?;
        SavedData::enforce_history_size_with_transaction(&mut transaction)?;
        transaction.commit()?;

        Ok(())
    }
}

pub fn validate_max_history_size(value: i64) -> Result<(), String> {
    if value < 1 {
        return Err("Maximum history size must be at least 1".to_string());
    }
    Ok(())
}
