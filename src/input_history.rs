use crate::error::InternalCalculatorError;
use crate::saved_data::SavedData;

/// The input history effectively keeps three instances of the history of user input entries.
/// Two are what we will call "primary" histories. These are only changed when inserting items. We
/// do not change the values of the strings that have been inserted.
/// The remaining copy we will call the "current" history. This is the copy that we navigate through
/// when the user is using the history scrollback. When the user makes changes to a historical
/// input, we change the version in the current history, leaving the version in the primary history
/// unchanged.
///
/// Once the user has finished editing/composing an input line and hits the return key,
/// `input_finished` ought to be called, which adds the current input to the primary history and
///  resets the current history to match the primaries. Additionally, one empty string is added to
/// the end of the current history, since the user will be composing a new input by default rather
/// than editing a historical one.
///
/// The histories are all structured as `Vec`s but they do not all sort the history in the same
/// order. See the definitions of the history data in the `InputHistory` definition for details.
pub struct InputHistory {
    /// This is the history of inputs that the user has entered during the current bcalc session.
    /// Its oldest entry will be at index `0`. When `input_finished` is called the current line of
    /// input will be appended to this history, but it otherwise will not be modified.
    primary_internal_history: Vec<String>,
    /// This is the history of inputs that the user has entered during previous bcalc sessions. The
    /// most recent entry will be at index `0`. It gets populated lazily, starting empty and having
    /// items added in from the database as they are requested.
    /// This history won't be used if `maybe_db` is `None`.
    primary_db_history: Vec<String>,
    /// This is the current history, which remembers changes made during the current line of input
    /// (i.e. between `input_finished` calls). It is sparse in two different ways. It always starts
    /// at length `1`, containing just the empty string that the input line defaults to. As the user
    /// navigates through the scrollback, the history is populated. But until changes are made to an
    /// input, they are stored as `None`. When we need to retrieve their values, we fetch them from
    /// the relevant primary history. Once changes are made, we clone the string into the current
    /// history so that we can make changes to it without modifying the primary history.
    current_history: Vec<Option<String>>,
    /// This tracks our current position in the histories, which is to say, what element of the
    /// history is currently being displayed to the user. It most directly represents the index that
    /// we are currently at in `current_history`. But it also indicates our position in the primary
    /// histories, it just requires a bit of math to determine where exactly. Index `0` in
    /// `current_history` is the initially-empty "composition" input that isn't an entry in either
    /// primary history. Then come the indicies that correspond to `primary_internal_history`
    /// entries. After those come the indicies that correspond to `primary_db_history` indicies.
    current_index: usize,
    /// Will be `true` if, when attempting to read an earlier entry from the database, we discovered
    /// that there wasn't one. There isn't any reason that an earlier entry would suddenly exist if
    /// we check again, so once this is `true`, we no longer attempt to read from the database.
    /// This will always be `true` if `InputHistory::new` was passed `false` for `use_db`.
    db_history_exhausted: bool,
}

impl InputHistory {
    pub fn new(use_db: bool) -> InputHistory {
        InputHistory {
            primary_internal_history: Vec::new(),
            primary_db_history: Vec::new(),
            current_history: vec![Some(String::new())],
            current_index: 0,
            db_history_exhausted: !use_db,
        }
    }

    /// Indicates that we are done editing/composing the current line of input. See the docstring
    /// for `InputHistory` for details.
    /// If `SavedData` is available, this will store the `current_line` to the history in the
    /// database. The function will then return the `id` of the inserted row.
    /// If `SavedData` is not available, this function will always return `Ok(None)`.
    pub fn input_finished(
        &mut self,
        maybe_db: Option<&mut SavedData>,
    ) -> Result<Option<i64>, Box<dyn std::error::Error>> {
        self.primary_internal_history
            .push(self.current_line().to_string());
        self.current_history.clear();
        self.current_history.push(Some(String::new()));
        self.current_index = 0;

        if let Some(db) = maybe_db {
            Ok(Some(db.add_to_input_history(
                &self.primary_internal_history[self.primary_internal_history.len() - 1],
            )?))
        } else {
            Ok(None)
        }
    }

    /// Returns the current line selected in the history (what the user should see).
    pub fn current_line(&self) -> &str {
        match &self.current_history[self.current_index] {
            Some(item) => item,
            None => {
                if self.current_index <= self.primary_internal_history.len() {
                    &self.primary_internal_history
                        [self.primary_internal_history.len() - self.current_index]
                } else {
                    &self.primary_db_history
                        [self.current_index - self.primary_internal_history.len() - 1]
                }
            }
        }
    }

    /// Attempts to move what line is the `current_line` to one line earlier in the history. If we
    /// are at the earliest entry in the input history, we may attempt to load an earlier entry from
    /// the database if it is available.
    /// Returns `Ok(true)` if `current_line` changed. Returns `Ok(false)` if there are no earlier
    /// entries to load.
    pub fn try_to_go_to_earlier_line(
        &mut self,
        maybe_db: Option<&mut SavedData>,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        // If we are at the earliest item in the history, attempt to load a newer one from the db.
        if self.current_index >= self.primary_internal_history.len() + self.primary_db_history.len()
        {
            if self.db_history_exhausted {
                return Ok(false);
            }
            let db = match maybe_db {
                Some(d) => d,
                None => {
                    return Err(InternalCalculatorError::new(concat!(
                        "InputHistory constructed with `use_db = true`, but ",
                        "`try_to_go_to_earlier_line` was not passed a database."
                    ))
                    .into())
                }
            };
            match db.get_prev_input_history()? {
                Some(input) => self.primary_db_history.push(input),
                None => {
                    self.db_history_exhausted = true;
                    return Ok(false);
                }
            }
        }

        if self.current_index + 1 >= self.current_history.len() {
            self.current_history.push(None);
        }
        self.current_index += 1;
        return Ok(true);
    }

    /// Attempts to move what line is the `current_line` to one line later in the history.
    /// Returns `true` if the `current_line` changed. Returns `false` if we are already at the
    /// latest entry in the history.
    pub fn try_to_go_to_later_line(&mut self) -> bool {
        if self.current_index <= 0 {
            return false;
        }
        self.current_index -= 1;
        return true;
    }

    /// Ensures that `self.current_history[self.current_index]` is `Some`.
    fn ensure_current_line_populated(&mut self) {
        if self.current_history[self.current_index].is_none() {
            self.current_history[self.current_index] = Some(self.current_line().to_string());
        }
    }

    /// Inserts the given character into the `current_line` at the `index` provided. The caller must
    /// ensure that a valid index is provided.
    pub fn insert_char_into_current_line(&mut self, index: usize, ch: char) {
        self.ensure_current_line_populated();
        self.current_history[self.current_index]
            .as_mut()
            .unwrap()
            .insert(index, ch);
    }

    /// Removes the character at the given `index` of the `current_line`. The caller must ensure
    /// that a valid index is provided.
    pub fn remove_char_from_current_line(&mut self, index: usize) {
        self.ensure_current_line_populated();
        self.current_history[self.current_index]
            .as_mut()
            .unwrap()
            .remove(index);
    }
}
