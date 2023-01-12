use crate::{
    error::{
        CalculatorFailure::{self, InputError},
        MissingCapabilityError,
    },
    input_history::InputHistory,
    position::{MaybePositioned, Positioned},
    saved_data::{validate_max_history_size, SavedData},
    token::Tokenizer,
    variable::VariableStore,
    Args,
};
use std::{
    cmp::max,
    collections::{HashMap, HashSet},
};

// When a new command is created, the constructor function needs to be added to this list.
const COMMAND_CONSTRUCTORS: &'static [fn() -> Box<dyn Command>] = &[
    HelpCommand::new,
    ReloadVarCommand::new,
    PurgeVarCommand::new,
    HistoryCapacityCommand::new,
];

struct DataForCommands<'a> {
    // TODO: Maybe remove lint override? I want this in here for now because I think I may add
    //       commands that need it later.
    #[allow(dead_code)]
    args: &'a mut Args,
    tokenizer: &'a Tokenizer,
    maybe_db: Option<&'a mut SavedData>,
    // TODO: Maybe remove lint override? I want this in here for now because I think I may add
    //       commands that need it later.
    #[allow(dead_code)]
    maybe_inputs: Option<&'a mut InputHistory>,
    maybe_vars: Option<&'a mut VariableStore>,
    command_map: &'a HashMap<String, Box<dyn Command>>,
}

trait Command {
    fn name(&self) -> &'static str;

    fn short_help(&self, data: &DataForCommands) -> String;

    fn long_help(&self, data: &DataForCommands) -> String;

    /// Returns an output string and a vector of variables touched by the command
    fn execute(
        &self,
        command_name: Positioned<String>,
        arguments: Positioned<String>,
        data: DataForCommands,
    ) -> Result<(String, Vec<String>), CalculatorFailure>;
}

pub struct CommandExecutor {
    command_map: HashMap<String, Box<dyn Command>>,
}

impl CommandExecutor {
    pub fn new() -> CommandExecutor {
        let mut command_map: HashMap<String, Box<dyn Command>> = HashMap::new();
        for constructor in COMMAND_CONSTRUCTORS {
            let command = constructor();
            command_map.insert(command.name().to_string(), command);
        }

        CommandExecutor { command_map }
    }

    pub fn execute_command(
        &mut self,
        command_name: Positioned<String>,
        arguments: Positioned<String>,
        program_arguments: &mut Args,
        tokenizer: &Tokenizer,
        maybe_db: Option<&mut SavedData>,
        maybe_inputs: Option<&mut InputHistory>,
        maybe_vars: Option<&mut VariableStore>,
    ) -> Result<(String, Vec<String>), CalculatorFailure> {
        match self.command_map.get(&command_name.value) {
            Some(command) => {
                let data = DataForCommands {
                    args: program_arguments,
                    tokenizer,
                    maybe_db,
                    maybe_inputs,
                    maybe_vars,
                    command_map: &self.command_map,
                };
                command.execute(command_name, arguments, data)
            }
            None => Err(InputError(MaybePositioned::new_positioned(
                format!("No such command: '{}'", command_name.value),
                command_name.position,
            ))),
        }
    }
}

struct HelpCommand;

impl HelpCommand {
    fn new() -> Box<dyn Command> {
        Box::new(HelpCommand {})
    }
}

impl Command for HelpCommand {
    fn name(&self) -> &'static str {
        "help"
    }

    fn short_help(&self, _data: &DataForCommands) -> String {
        "Gives help with commands".to_string()
    }

    fn long_help(&self, _data: &DataForCommands) -> String {
        concat!(
            "Usage: /help\n",
            "       /help command_name\n\n",
            "With no arguments, lists all the available commands. If a command is given as an ",
            "argument, provides more detailed help with the specified command."
        )
        .to_string()
    }

    fn execute(
        &self,
        _command_name: Positioned<String>,
        mut arguments: Positioned<String>,
        data: DataForCommands,
    ) -> Result<(String, Vec<String>), CalculatorFailure> {
        arguments.trim();
        let command_name = arguments;

        if command_name.value.is_empty() {
            let mut commands: Vec<String> = data.command_map.keys().cloned().collect();
            commands.sort();
            let max_command_width = commands
                .iter()
                .fold(0, |acc, command| max(acc, command.len()));

            let mut output = String::new();
            output.push_str("Available commands:");
            for command in commands {
                output.push_str(&format!("\n  {:max_command_width$} ", &command));
                output.push_str(&data.command_map[&command].short_help(&data));
            }
            Ok((output, Vec::new()))
        } else {
            match data.command_map.get(&command_name.value) {
                Some(command) => Ok((command.long_help(&data), Vec::new())),
                None => {
                    return Err(InputError(MaybePositioned::new_positioned(
                        format!("No such command: '{}'", command_name.value),
                        command_name.position,
                    )))
                }
            }
        }
    }
}

struct ReloadVarCommand;

impl ReloadVarCommand {
    fn new() -> Box<dyn Command> {
        Box::new(ReloadVarCommand {})
    }
}

impl Command for ReloadVarCommand {
    fn name(&self) -> &'static str {
        "reloadvar"
    }

    fn short_help(&self, data: &DataForCommands) -> String {
        let mut output = String::new();
        if data.maybe_db.is_none() || data.maybe_vars.is_none() {
            output.push_str("(unavailable) ");
        }
        output.push_str("Reloads variable(s) from the disk");

        output
    }

    fn long_help(&self, data: &DataForCommands) -> String {
        let mut output = concat!(
            "Usage: /reloadvar variable_name_1 [variable_name_2 [...]]\n\n",
            "Reloads the specified variable(s) from the variable history in the on-disk ",
            "database. Fails with no effect if there is no such variable in the database."
        )
        .to_string();
        if data.maybe_db.is_none() || data.maybe_vars.is_none() {
            output.push_str("\n\nThis command is currently unavailable because ");
            if data.maybe_db.is_none() && data.maybe_vars.is_none() {
                output.push_str("both the on-disk database and the variable store are");
            } else if data.maybe_db.is_none() {
                output.push_str("the on-disk database is");
            } else if data.maybe_vars.is_none() {
                output.push_str("the variable store is");
            }
            output.push_str(" unavailable.");
        }

        output
    }

    fn execute(
        &self,
        _command_name: Positioned<String>,
        arguments: Positioned<String>,
        data: DataForCommands,
    ) -> Result<(String, Vec<String>), CalculatorFailure> {
        let variable_tokens: HashSet<Positioned<String>> = data
            .tokenizer
            .tokenize_variable_list(&arguments.value)?
            .into_iter()
            .collect();

        let db = data.maybe_db.ok_or(MissingCapabilityError::NoDatabase)?;
        let vars = data
            .maybe_vars
            .ok_or(MissingCapabilityError::NoVariableStore)?;

        let mut output = String::new();
        let mut variables_touched: Vec<String> = Vec::new();
        for variable_token in variable_tokens {
            if !output.is_empty() {
                output.push('\n');
            }
            if let Some(reloaded) = vars.reload(variable_token.value.clone(), db)? {
                output.push_str(&format!("Set {} to {}", reloaded.name, reloaded.value));
            } else {
                output.push_str(&format!("{} unchanged", variable_token.value));
            }
            variables_touched.push(variable_token.value);
        }

        Ok((output, variables_touched))
    }
}

struct PurgeVarCommand;

impl PurgeVarCommand {
    fn new() -> Box<dyn Command> {
        Box::new(PurgeVarCommand {})
    }
}

impl Command for PurgeVarCommand {
    fn name(&self) -> &'static str {
        "purgevar"
    }

    fn short_help(&self, data: &DataForCommands) -> String {
        let mut output = String::new();
        if data.maybe_vars.is_none() {
            output.push_str("(unavailable) ");
        }
        output.push_str("Unsets variable(s)");

        output
    }

    fn long_help(&self, data: &DataForCommands) -> String {
        let mut output = concat!(
            "Usage: /purgevar variable_name_1 [variable_name_2 [...]]\n\n",
            "Removes the variable(s) from both the variable store and the variable history in the ",
            "on-disk database, if available."
        )
        .to_string();
        if data.maybe_vars.is_none() {
            output.push_str(concat!(
                "\n\nThis command is currently unavailable because the variable store is ",
                "unavailable."
            ));
        }

        output
    }

    fn execute(
        &self,
        _command_name: Positioned<String>,
        arguments: Positioned<String>,
        mut data: DataForCommands,
    ) -> Result<(String, Vec<String>), CalculatorFailure> {
        let variable_tokens: HashSet<Positioned<String>> = data
            .tokenizer
            .tokenize_variable_list(&arguments.value)?
            .into_iter()
            .collect();

        let vars = data
            .maybe_vars
            .ok_or(MissingCapabilityError::NoVariableStore)?;

        for variable_token in variable_tokens {
            // `as_deref_mut` is used here to reborrow the database reference into a new `Option`.
            // If we didn't do that, we would move `data.maybe_db` into the `purge` call and then
            // wouldn't be able to call it again when we loop.
            vars.purge(&variable_token.value, data.maybe_db.as_deref_mut())?;
        }

        // Technically this touches variables, but it also removes them. Which means that reporting
        // them as touched isn't really meaningful.
        Ok(("Done".to_string(), Vec::new()))
    }
}

struct HistoryCapacityCommand;

impl HistoryCapacityCommand {
    fn new() -> Box<dyn Command> {
        Box::new(HistoryCapacityCommand {})
    }
}

impl Command for HistoryCapacityCommand {
    fn name(&self) -> &'static str {
        "histcap"
    }

    fn short_help(&self, data: &DataForCommands) -> String {
        let mut output = String::new();
        if data.maybe_db.is_none() {
            output.push_str("(unavailable) ");
        }
        output.push_str("Get/Set the on-disk history capacity");

        output
    }

    fn long_help(&self, data: &DataForCommands) -> String {
        let mut output = concat!(
            "Usage: /histcap [size]\n\n",
            "If a size is provided, the maximum on-disk history size is updated. If one is not ",
            "provided, the current size is printed.\n",
            "bcalc provides an in-memory input history as well as an on-disk one. The in-memory ",
            "history is currently not limited but will, of course, be lost when the instance of ",
            "bcalc exits. The on-disk history is not cleared when bcalc exits. It would be a ",
            "problem for it to grow out of control, so it is limited to a certain number of ",
            "inputs. After reaching the limit, old entries will be removed.\n",
            "The variable history is also tied to the input history. Values will be removed from ",
            "the variable history after the last input that accessed that value is removed from ",
            "the input history.\n",
            "Provided size will always be assumed to use radix (base) 10.",
        )
        .to_string();
        if data.maybe_db.is_none() {
            output.push_str(concat!(
                "\n\nThis command is currently unavailable because the on-disk database is ",
                "unavailable."
            ));
        }

        output
    }

    fn execute(
        &self,
        _command_name: Positioned<String>,
        arguments: Positioned<String>,
        data: DataForCommands,
    ) -> Result<(String, Vec<String>), CalculatorFailure> {
        let mut parsed_args = data.tokenizer.tokenize_int_list(&arguments.value, 10)?;
        let input: Option<i64> = if parsed_args.is_empty() {
            None
        } else if parsed_args.len() == 1 {
            let integer = parsed_args.pop().unwrap();
            validate_max_history_size(integer.value)
                .map_err(|s| InputError(MaybePositioned::new_positioned(s, integer.position)))?;
            Some(integer.value)
        } else {
            let last_arg = parsed_args.pop().unwrap();
            let first_arg = parsed_args.into_iter().next().unwrap();
            return Err(InputError(MaybePositioned::new_span(
                "Too many arguments".to_string(),
                first_arg.position,
                last_arg.position,
            )));
        };

        let db = data.maybe_db.ok_or(MissingCapabilityError::NoDatabase)?;

        match input {
            Some(size) => {
                db.set_max_history_size(size)?;
                Ok(("Done".to_string(), Vec::new()))
            }
            None => {
                let capacity = db.get_max_history_size()?;
                Ok((capacity.to_string(), Vec::new()))
            }
        }
    }
}
