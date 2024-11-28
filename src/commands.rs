use crate::{
    error::{
        CalculatorFailure::{self, InputError},
        MissingCapabilityError,
    },
    input_history::InputHistory,
    position::{MaybePositioned, Position, Positioned},
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
    FractionalCommand::new,
    RadixCommand::new,
    ConvertToRadixCommand::new,
    UpperCommand::new,
    CommaCommand::new,
    PrecisionCommand::new,
];

struct DataForCommands<'a> {
    args: &'a mut Args,
    tokenizer: &'a Tokenizer,
    maybe_db: Option<&'a mut SavedData>,
    // TODO: Maybe remove lint override? I want this in here for now because I think I may add
    //       commands that need it later.
    #[allow(dead_code)]
    maybe_inputs: Option<&'a mut InputHistory>,
    maybe_vars: Option<&'a mut VariableStore>,
    command_map: &'a HashMap<String, Box<dyn Command>>,
    alias_map: &'a HashMap<String, String>,
}

trait Command {
    fn name(&self) -> &'static str;

    fn aliases(&self) -> &'static [&'static str];

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
    alias_map: HashMap<String, String>,
}

impl CommandExecutor {
    pub fn new() -> CommandExecutor {
        let mut command_map: HashMap<String, Box<dyn Command>> = HashMap::new();
        let mut alias_map: HashMap<String, String> = HashMap::new();
        for constructor in COMMAND_CONSTRUCTORS {
            let command = constructor();
            let command_name = command.name().to_string();
            for alias in command.aliases() {
                let alias_string = alias.to_string();
                if command_map.get(&alias_string).is_some() {
                    panic!("Alias matches command: {}", alias);
                }
                if alias_map
                    .insert(alias_string, command_name.clone())
                    .is_some()
                {
                    panic!("Duplicate alias: {}", alias);
                }
            }
            if alias_map.get(&command_name).is_some() {
                panic!("Command matches alias: {}", command_name);
            }
            if let Some(replaced_command) = command_map.insert(command_name, command) {
                panic!("Duplicate command: {}", replaced_command.name());
            }
        }

        CommandExecutor {
            command_map,
            alias_map,
        }
    }

    pub fn execute_command(
        &mut self,
        alias_name: Positioned<String>,
        arguments: Positioned<String>,
        program_arguments: &mut Args,
        tokenizer: &Tokenizer,
        maybe_db: Option<&mut SavedData>,
        maybe_inputs: Option<&mut InputHistory>,
        maybe_vars: Option<&mut VariableStore>,
    ) -> Result<(String, Vec<String>), CalculatorFailure> {
        let command_name = match self.alias_map.get(&alias_name.value) {
            Some(name) => name,
            None => &alias_name.value,
        };

        match self.command_map.get(command_name) {
            Some(command) => {
                let data = DataForCommands {
                    args: program_arguments,
                    tokenizer,
                    maybe_db,
                    maybe_inputs,
                    maybe_vars,
                    command_map: &self.command_map,
                    alias_map: &self.alias_map,
                };
                command.execute(alias_name, arguments, data)
            }
            None => Err(InputError(MaybePositioned::new_positioned(
                format!("No such command: '{}'", alias_name.value),
                alias_name.position,
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

    fn aliases(&self) -> &'static [&'static str] {
        &["h"]
    }

    fn short_help(&self, _data: &DataForCommands) -> String {
        "Gives help with commands".to_string()
    }

    fn long_help(&self, _data: &DataForCommands) -> String {
        concat!(
            "Usage: /help\n",
            "       /help command_name\n",
            "Alias: /h\n\n",
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
        let alias_name = arguments;

        if alias_name.value.is_empty() {
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
            let command_name = match data.alias_map.get(&alias_name.value) {
                Some(name) => name,
                None => &alias_name.value,
            };

            match data.command_map.get(command_name) {
                Some(command) => Ok((command.long_help(&data), Vec::new())),
                None => {
                    return Err(InputError(MaybePositioned::new_positioned(
                        format!("No such command: '{}'", alias_name.value),
                        alias_name.position,
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

    fn aliases(&self) -> &'static [&'static str] {
        &[]
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

    fn aliases(&self) -> &'static [&'static str] {
        &[]
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

    fn aliases(&self) -> &'static [&'static str] {
        &[]
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

struct FractionalCommand;

impl FractionalCommand {
    fn new() -> Box<dyn Command> {
        Box::new(FractionalCommand {})
    }
}

impl Command for FractionalCommand {
    fn name(&self) -> &'static str {
        "fractional"
    }

    fn aliases(&self) -> &'static [&'static str] {
        &["f"]
    }

    fn short_help(&self, _data: &DataForCommands) -> String {
        "Retrieves or sets fractional display setting".to_string()
    }

    fn long_help(&self, _data: &DataForCommands) -> String {
        concat!(
            "Usage: /fractional [enabled]\n",
            "Alias: /f\n\n",
            "If the enabled value is \"true\", non-integer numbers will be output as fractions. ",
            "If the value is \"false\", non-integer numbers will be output as decimals.\n",
            "If no value is provided, the current setting value is displayed.\n",
            "If a value is given, the setting value is updated.\n",
            "The value given should be a boolean, which can be represented as \"true\", ",
            "\"false\", \"t\", or \"f\".",
        )
        .to_string()
    }

    fn execute(
        &self,
        _command_name: Positioned<String>,
        arguments: Positioned<String>,
        data: DataForCommands,
    ) -> Result<(String, Vec<String>), CalculatorFailure> {
        let arg_lower = arguments.value.to_lowercase();
        let arg_string = arg_lower.trim();
        if arg_string.is_empty() {
            return Ok((format!("{}", data.args.fractional), Vec::new()));
        }

        let value = if arg_string == "f" || arg_string == "false" {
            false
        } else if arg_string == "t" || arg_string == "true" {
            true
        } else {
            return Err(InputError(MaybePositioned::new_positioned(
                "Invalid argument".to_string(),
                arguments.position,
            )));
        };

        data.args.fractional = value;
        Ok(("Done".to_string(), Vec::new()))
    }
}

struct RadixCommand;

impl RadixCommand {
    fn new() -> Box<dyn Command> {
        Box::new(RadixCommand {})
    }
}

impl Command for RadixCommand {
    fn name(&self) -> &'static str {
        "radix"
    }

    fn aliases(&self) -> &'static [&'static str] {
        &[]
    }

    fn short_help(&self, _data: &DataForCommands) -> String {
        "Retrieves or sets the current radix".to_string()
    }

    fn long_help(&self, _data: &DataForCommands) -> String {
        concat!(
            "Usage: /radix [value]\n\n",
            "Value represents the radix used to parse and output numbers.\n",
            "If no value is provided, the current setting value is displayed.\n",
            "If a value is given, the setting value is updated.\n",
            "The value given should be an integer between 2 and 16 (inclusive).",
        )
        .to_string()
    }

    fn execute(
        &self,
        _command_name: Positioned<String>,
        arguments: Positioned<String>,
        data: DataForCommands,
    ) -> Result<(String, Vec<String>), CalculatorFailure> {
        let mut parsed_args = data.tokenizer.tokenize_int_list(&arguments.value, 10)?;
        let input: Option<u8> = if parsed_args.is_empty() {
            None
        } else if parsed_args.len() == 1 {
            let integer = parsed_args.pop().unwrap();
            if integer.value < 2 {
                return Err(InputError(MaybePositioned::new_positioned(
                    "Radix cannot be less than 2".to_string(),
                    integer.position,
                )));
            }
            if integer.value > 16 {
                return Err(InputError(MaybePositioned::new_positioned(
                    "Radix cannot be greater than 16".to_string(),
                    integer.position,
                )));
            }
            Some(integer.value.try_into().unwrap())
        } else {
            let last_arg = parsed_args.pop().unwrap();
            let first_arg = parsed_args.into_iter().next().unwrap();
            return Err(InputError(MaybePositioned::new_span(
                "Too many arguments".to_string(),
                first_arg.position,
                last_arg.position,
            )));
        };

        match input {
            Some(value) => {
                data.args.radix = value;
                Ok(("Done".to_string(), Vec::new()))
            }
            None => Ok((format!("{}", data.args.radix), Vec::new())),
        }
    }
}

struct ConvertToRadixCommand;

impl ConvertToRadixCommand {
    fn new() -> Box<dyn Command> {
        Box::new(ConvertToRadixCommand {})
    }
}

impl Command for ConvertToRadixCommand {
    fn name(&self) -> &'static str {
        "converttoradix"
    }

    fn aliases(&self) -> &'static [&'static str] {
        &[]
    }

    fn short_help(&self, _data: &DataForCommands) -> String {
        "Retrieves or sets the current output radix".to_string()
    }

    fn long_help(&self, _data: &DataForCommands) -> String {
        concat!(
            "Usage: /converttoradix [value]\n\n",
            "Value overrides the radix used to output numbers.\n",
            "If no value is provided, the current setting value is displayed.\n",
            "If a value is given, the setting value is updated.\n",
            "The value given can be \"none\" or an integer between 2 and 16 (inclusive).",
        )
        .to_string()
    }

    fn execute(
        &self,
        _command_name: Positioned<String>,
        arguments: Positioned<String>,
        data: DataForCommands,
    ) -> Result<(String, Vec<String>), CalculatorFailure> {
        // "none" is a valid input, but won't be tokenized successfully. So handle that possibility
        // first.
        if arguments.value.to_lowercase().trim() == "none" {
            data.args.convert_to_radix = None;
            return Ok(("Done".to_string(), Vec::new()));
        }

        let mut parsed_args = data.tokenizer.tokenize_int_list(&arguments.value, 10)?;
        let input: Option<u8> = if parsed_args.is_empty() {
            None
        } else if parsed_args.len() == 1 {
            let integer = parsed_args.pop().unwrap();
            if integer.value < 2 {
                return Err(InputError(MaybePositioned::new_positioned(
                    "Radix cannot be less than 2".to_string(),
                    integer.position,
                )));
            }
            if integer.value > 16 {
                return Err(InputError(MaybePositioned::new_positioned(
                    "Radix cannot be greater than 16".to_string(),
                    integer.position,
                )));
            }
            Some(integer.value.try_into().unwrap())
        } else {
            let last_arg = parsed_args.pop().unwrap();
            let first_arg = parsed_args.into_iter().next().unwrap();
            return Err(InputError(MaybePositioned::new_span(
                "Too many arguments".to_string(),
                first_arg.position,
                last_arg.position,
            )));
        };

        match input {
            Some(value) => {
                data.args.convert_to_radix = Some(value);
                Ok(("Done".to_string(), Vec::new()))
            }
            None => match data.args.convert_to_radix {
                Some(radix) => Ok((format!("{}", radix), Vec::new())),
                None => Ok(("None".to_string(), Vec::new())),
            },
        }
    }
}

struct UpperCommand;

impl UpperCommand {
    fn new() -> Box<dyn Command> {
        Box::new(UpperCommand {})
    }
}

impl Command for UpperCommand {
    fn name(&self) -> &'static str {
        "upper"
    }

    fn aliases(&self) -> &'static [&'static str] {
        &[]
    }

    fn short_help(&self, _data: &DataForCommands) -> String {
        "Retrieves or sets upper display setting".to_string()
    }

    fn long_help(&self, _data: &DataForCommands) -> String {
        concat!(
            "Usage: /upper [enabled]\n\n",
            "If the enabled value is \"true\", digits above 9 will be output in uppercase. If ",
            "\"false\", they will be output in lowercase.\n",
            "If no value is provided, the current setting value is displayed.\n",
            "If a value is given, the setting value is updated.\n",
            "The value given should be a boolean, which can be represented as \"true\", ",
            "\"false\", \"t\", or \"f\".",
        )
        .to_string()
    }

    fn execute(
        &self,
        _command_name: Positioned<String>,
        arguments: Positioned<String>,
        data: DataForCommands,
    ) -> Result<(String, Vec<String>), CalculatorFailure> {
        let arg_lower = arguments.value.to_lowercase();
        let arg_string = arg_lower.trim();
        if arg_string.is_empty() {
            return Ok((format!("{}", data.args.upper), Vec::new()));
        }

        let value = if arg_string == "f" || arg_string == "false" {
            false
        } else if arg_string == "t" || arg_string == "true" {
            true
        } else {
            return Err(InputError(MaybePositioned::new_positioned(
                "Invalid argument".to_string(),
                arguments.position,
            )));
        };

        data.args.upper = value;
        Ok(("Done".to_string(), Vec::new()))
    }
}

struct CommaCommand;

impl CommaCommand {
    fn new() -> Box<dyn Command> {
        Box::new(CommaCommand {})
    }
}

impl Command for CommaCommand {
    fn name(&self) -> &'static str {
        "commas"
    }

    fn aliases(&self) -> &'static [&'static str] {
        &[]
    }

    fn short_help(&self, _data: &DataForCommands) -> String {
        "Retrieves or sets comma display setting".to_string()
    }

    fn long_help(&self, _data: &DataForCommands) -> String {
        concat!(
            "Usage: /commas [enabled]\n\n",
            "If the enabled value is \"true\", commas will be used as thousands separators when ",
            "outputting numbers.\n",
            "If no value is provided, the current setting value is displayed.\n",
            "If a value is given, the setting value is updated.\n",
            "The value given should be a boolean, which can be represented as \"true\", ",
            "\"false\", \"t\", or \"f\".",
        )
        .to_string()
    }

    fn execute(
        &self,
        _command_name: Positioned<String>,
        arguments: Positioned<String>,
        data: DataForCommands,
    ) -> Result<(String, Vec<String>), CalculatorFailure> {
        let arg_lower = arguments.value.to_lowercase();
        let arg_string = arg_lower.trim();
        if arg_string.is_empty() {
            return Ok((format!("{}", data.args.commas), Vec::new()));
        }

        let value = if arg_string == "f" || arg_string == "false" {
            false
        } else if arg_string == "t" || arg_string == "true" {
            true
        } else {
            return Err(InputError(MaybePositioned::new_positioned(
                "Invalid argument".to_string(),
                arguments.position,
            )));
        };

        data.args.commas = value;
        Ok(("Done".to_string(), Vec::new()))
    }
}

struct PrecisionCommand;

impl PrecisionCommand {
    fn new() -> Box<dyn Command> {
        Box::new(PrecisionCommand {})
    }
}

impl Command for PrecisionCommand {
    fn name(&self) -> &'static str {
        "precision"
    }

    fn aliases(&self) -> &'static [&'static str] {
        &["p"]
    }

    fn short_help(&self, _data: &DataForCommands) -> String {
        "Retrieves or sets the current precision".to_string()
    }

    fn long_help(&self, _data: &DataForCommands) -> String {
        concat!(
            "Usage: /precision [value [extra]]\n\n",
            "The value represents the maximum number of digits that are displayed after the ",
            "decimal point when outputting numbers.\n",
            "If no value is provided, the current setting value is displayed.\n",
            "If a value is given, the setting value is updated.\n",
            "The value given should be representable as an 8-bit unsigned integer.\n",
            "If extra is given, it should also be representable as an 8-bit unsigned integer.\n",
            "This will represent the additional precision that is stored internally but not displayed.\n",
            "This is only really relevant for operations that cannot be done with infinite precision.\n",
            "For example: sqrt(2)\n",
            "value + extra must also be representable as an 8-bit unsigned integer."
        )
        .to_string()
    }

    fn execute(
        &self,
        _command_name: Positioned<String>,
        arguments: Positioned<String>,
        data: DataForCommands,
    ) -> Result<(String, Vec<String>), CalculatorFailure> {
        let mut parsed_args = data.tokenizer.tokenize_int_list(&arguments.value, 10)?;
        let input: Option<(u8, u8)> = if parsed_args.is_empty() {
            None
        } else if parsed_args.len() <= 2 {
            let mut parsed_args_iter = parsed_args.into_iter();
            let precision_raw = parsed_args_iter.next().unwrap();
            let precision: u8 = precision_raw.value.try_into().map_err(|_| {
                InputError(MaybePositioned::new_positioned(
                    "Precision must be representable as an 8-bit unsigned integer".to_string(),
                    precision_raw.position.clone(),
                ))
            })?;
            let maybe_extra = parsed_args_iter.next();
            let extra: u8 = match &maybe_extra {
                None => data.args.extra_precision,
                Some(extra_raw) => extra_raw.value.try_into().map_err(|_| {
                    InputError(MaybePositioned::new_positioned(
                        "Extra must be representable as an 8-bit unsigned integer".to_string(),
                        extra_raw.position.clone(),
                    ))
                })?,
            };

            if precision.checked_add(extra).is_none() {
                let position = match maybe_extra {
                    None => precision_raw.position,
                    Some(extra_raw) => {
                        Position::from_span(precision_raw.position, extra_raw.position)
                    }
                };
                return Err(InputError(MaybePositioned::new_positioned(
                    "Sum of precision and extra must be representable as an 8-bit unsigned integer"
                        .to_string(),
                    position,
                )));
            }

            Some((precision, extra))
        } else {
            let last_arg = parsed_args.pop().unwrap();
            let first_arg = parsed_args.into_iter().next().unwrap();
            return Err(InputError(MaybePositioned::new_span(
                "Too many arguments".to_string(),
                first_arg.position,
                last_arg.position,
            )));
        };

        match input {
            Some((precision, extra)) => {
                data.args.precision = precision;
                data.args.extra_precision = extra;
                Ok(("Done".to_string(), Vec::new()))
            }
            None => Ok((
                format!(
                    "Precision = {}\nExtra Precision = {}",
                    data.args.precision, data.args.extra_precision
                ),
                Vec::new(),
            )),
        }
    }
}
