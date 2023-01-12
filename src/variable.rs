use crate::{error::InternalCalculatorError, saved_data::SavedData};
use num::rational::BigRational;
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct Variable {
    pub name: String,
    pub value: BigRational,
}

/// `VariableStore` may be constructed with or without access to `SavedData`. In either case,
/// we store the variables internally. But if we have `SavedData`, we also write them out to the
/// database. We also load them from the database, but only if we don't have that variable
/// internally.
pub struct VariableStore {
    vars: HashMap<String, BigRational>,
}

impl VariableStore {
    pub fn new() -> VariableStore {
        VariableStore {
            vars: HashMap::new(),
        }
    }

    /// Always updates the internal `VariableStore`. Returns an error if it fails to also update the
    /// database.
    /// If `VariableStore::new` was passed `Some` for `maybe_db` to construct this instance,
    /// `maybe_input_history_id` must be `Some` when this is called.
    pub fn update(
        &mut self,
        var: Variable,
        maybe_input_history_id: Option<i64>,
        maybe_db: Option<&mut SavedData>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let result = match (maybe_db, maybe_input_history_id) {
            (Some(db), Some(input_history_id)) => db.set_variable(&var, input_history_id),
            (Some(_), None) => Err(InternalCalculatorError::new(
                "VariableStore missing input history id when updating variable",
            )
            .into()),
            (None, Some(_)) => Err(InternalCalculatorError::new(concat!(
                "VariableStore was provided an input history id but no database when ",
                "updating variable"
            ))
            .into()),
            (None, None) => Ok(()),
        };

        self.vars.insert(var.name, var.value);

        result
    }

    pub fn touch(
        &mut self,
        name: &str,
        maybe_input_history_id: Option<i64>,
        maybe_db: Option<&mut SavedData>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        match (maybe_db, maybe_input_history_id) {
            (Some(db), Some(input_history_id)) => db.touch_variable(name, input_history_id),
            (Some(_), None) => Err(InternalCalculatorError::new(
                "VariableStore missing input history id when touching variable",
            )
            .into()),
            (None, Some(_)) => Err(InternalCalculatorError::new(concat!(
                "VariableStore was provided an input history id but no database when ",
                "touching variable"
            ))
            .into()),
            (None, None) => Ok(()),
        }
    }

    /// Returns the value in the instance's variable store. If the value isn't available, we attempt
    /// to populate the value from `SavedData` and return that.
    pub fn get(
        &mut self,
        name: String,
        maybe_db: Option<&mut SavedData>,
    ) -> Result<Option<Variable>, Box<dyn std::error::Error>> {
        if let Some(value) = self.vars.get(&name) {
            return Ok(Some(Variable {
                name: name,
                value: value.clone(),
            }));
        }

        if let Some(db) = maybe_db {
            self.reload(name, db)
        } else {
            Ok(None)
        }
    }

    // Attempts to load a variable from `SavedData`'s variable history and, if it exists, overwrites
    // any value in the instance's variable store. If the variable is not found in the variable
    // history, this has no effect and `Ok(None)` is returned.
    pub fn reload(
        &mut self,
        name: String,
        db: &mut SavedData,
    ) -> Result<Option<Variable>, Box<dyn std::error::Error>> {
        if let Some(var) = db.get_variable(name)? {
            self.vars.insert(var.name.clone(), var.value.clone());
            Ok(Some(var))
        } else {
            Ok(None)
        }
    }

    // Removes the variable from the instance's variable store. If `SavedData`'s variable history is
    // available, the variable is removed from it too.
    // `Ok` will be returned if the variable does not exist in either location, regardless of
    // whether or not it did before.
    pub fn purge(
        &mut self,
        name: &str,
        maybe_db: Option<&mut SavedData>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.vars.remove(name);

        if let Some(db) = maybe_db {
            db.clear_variable(name)?;
        }

        Ok(())
    }
}
