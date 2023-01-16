mod commands;
mod error;
mod input_history;
mod position;
mod saved_data;
mod syntax_tree;
mod token;
mod variable;

use clap::Parser;
use commands::CommandExecutor;
use crossterm::{
    cursor::{self, MoveTo, MoveToColumn, MoveToNextLine},
    event::{self, Event, KeyCode, KeyModifiers},
    execute, queue,
    style::Print,
    terminal::{
        self, Clear,
        ClearType::{CurrentLine, FromCursorDown},
        EnterAlternateScreen, LeaveAlternateScreen,
    },
};
use error::{CalculatorEnvironmentError, CalculatorFailure, InternalCalculatorError};
use input_history::InputHistory;
use saved_data::SavedData;
use std::{
    cmp::{max, min},
    collections::HashSet,
    io::{stdout, Write},
};
use syntax_tree::SyntaxTree;
use token::{ParsedInput, Token, Tokenizer};
use variable::VariableStore;

// `PROMPT_STR.len()` should equal `SCROLL_LEFT_INDICATOR_STR.len()`.
const PROMPT_STR: &str = "# ";
const SCROLL_LEFT_INDICATOR_STR: &str = "< ";
const SCROLL_RIGHT_INDICATOR_STR: &str = " >";

const LARGE_CURSOR_MOVE_DISTANCE: usize = 15;

#[derive(Parser, Clone, Debug)]
#[command(version, about, long_about = None)]
pub struct Args {
    /// Radix (base) to use for input and output.
    #[arg(short, long, default_value_t = 10)]
    #[arg(value_parser = clap::value_parser!(u8).range(1..17))]
    radix: u8,

    /// If specified, input will be read from the provided string rather than interactively.
    #[arg(short, long)]
    input: Option<String>,

    /// If specified, an alternate terminal screen is opened rather than doing the calculations
    /// inline. In this mode, entered calculations wrap rather than scrolling.
    #[arg(short, long)]
    alternate_screen: bool,

    /// Normally, the calculator attempts to load data such as input history from a user-specific
    /// database. If this option is specified, the database will not be used.
    #[arg(long)]
    no_db: bool,
    // TODO: Implement
    // If specified, the output radix (base) will be set to this rather than being the same as the
    // input radix.
    // #[arg(long)]
    // #[arg(value_parser = clap::value_parser!(u8).range(1..17))]
    // convert_to_radix: Option<u8>,

    // TODO: Implement
    // If specified, the output will use commas as thousands separators to make long numbers more
    // readable.
    // #[arg(short, long)]
    // commas: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = Args::parse();
    let mut command_executor = CommandExecutor::new();
    let tokenizer = Tokenizer::new();

    match args.input.clone() {
        Some(input) => {
            match calculate(
                &input,
                &mut args,
                &tokenizer,
                &mut command_executor,
                None,
                None,
                None,
            ) {
                Ok(result) => println!("{}", result),
                Err(CalculatorFailure::InputError(message)) => {
                    eprintln!("Error: {}", message.value)
                }
                Err(CalculatorFailure::RuntimeError(e)) => return Err(e),
            }
        }
        None => {
            let mut stdout = stdout();
            terminal::enable_raw_mode()?;
            if args.alternate_screen {
                if let Err(e) = execute!(stdout, EnterAlternateScreen) {
                    let _ = terminal::disable_raw_mode();
                    return Err(e.into());
                }
            }

            let result = interactive_calc(&mut args, command_executor, tokenizer);

            if args.alternate_screen {
                let _ = execute!(stdout, LeaveAlternateScreen);
            }
            let _ = terminal::disable_raw_mode();
            result?;
        }
    }

    Ok(())
}

// We want pretty fine-grained control over the calculator interface so that we can:
//  - Handle hotkey commands (ex: Control+M).
//  - Exit cleanly on Control+C, Control+D, and Control+Z.
//  - Allow both standard cursor movement and more advanced cursor movement via hotkeys.
//  - Provide access to history/scrollback.
// In order to do these things, we do need to reinvent the wheel somewhat. So this is going to be a
// bit ridiculous and over-engineered. But it accomplishes what I want.
fn interactive_calc(
    args: &mut Args,
    mut command_executor: CommandExecutor,
    tokenizer: Tokenizer,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut stdout = stdout();

    // If available, we are going to open an SQLite connection to bcalc's saved data file. This
    // will allow us to do things like having the scrollback extend to previous bcalc instances.
    let mut maybe_db: Option<SavedData> = if args.no_db { None } else { SavedData::open()? };
    let mut inputs = InputHistory::new(maybe_db.is_some());
    let mut vars = VariableStore::new();

    'calculate: loop {
        let mut cursor_pos: usize = 0;
        let mut scroll_offset: usize = 0;
        let input_start = cursor::position()?;
        let mut cols = usize::from(terminal::size()?.0);
        let mut input_complete = false;

        'get_input_line: loop {
            // We display before we process input so that the prompt shows up without user input.
            // If we are in the alternate screen or the input will not need to be edited anymore,
            // we will output the input line wrapped so that the user can read it all. If we are
            // still doing inline editing, we may not have any way of returning to previous lines
            // if we wrap, so we will instead allow the current line to scroll.
            let current_input = inputs.current_line();
            if args.alternate_screen || input_complete {
                let wrap_str: String = std::iter::repeat(" ").take(PROMPT_STR.len()).collect();
                if cols < wrap_str.len() {
                    return Err(CalculatorEnvironmentError::new("Window too narrow").into());
                }
                let available_cols = cols - wrap_str.len();
                let cursor_row: u16 = u16::try_from(cursor_pos / available_cols)? + input_start.1;
                let cursor_col: u16 =
                    u16::try_from((cursor_pos % available_cols) + wrap_str.len())?;
                let mut end_index = min(available_cols, current_input.len());
                if args.alternate_screen {
                    queue!(
                        stdout,
                        MoveTo(input_start.0, input_start.1),
                        Clear(FromCursorDown)
                    )?;
                } else {
                    queue!(stdout, MoveToColumn(0), Clear(CurrentLine))?;
                }
                // First display the prompt and as much text as we can fit on the first line. Then
                // loop over the remaining text, starting each subsequent line with `wrap_str`
                // until we have displayed the whole string.
                queue!(
                    stdout,
                    Print(PROMPT_STR),
                    Print(&current_input[0..end_index])
                )?;
                let mut current_index = end_index;
                while current_index < current_input.len() {
                    end_index = min(current_index + available_cols, current_input.len());
                    if args.alternate_screen {
                        queue!(stdout, MoveToNextLine(1))?;
                    } else {
                        // MoveToNextLine doesn't seem to always work properly if we aren't in the
                        // alternate screen.
                        queue!(stdout, Print("\n"))?;
                    }
                    queue!(
                        stdout,
                        Print(&wrap_str),
                        Print(&current_input[current_index..end_index])
                    )?;
                    current_index = end_index;
                }
                if input_complete {
                    if args.alternate_screen {
                        queue!(stdout, MoveToNextLine(1))?;
                    } else {
                        // MoveToNextLine doesn't seem to always work properly if we aren't in the
                        // alternate screen.
                        queue!(stdout, Print("\n"), MoveToColumn(0))?;
                    }
                } else {
                    queue!(stdout, MoveTo(cursor_col, cursor_row))?;
                }
                stdout.flush()?;
            } else {
                // Not in the alternate screen and still accepting input = scrolling behavior.

                // TODO: Is there some way of ensuring this at compile time?
                assert_eq!(PROMPT_STR.len(), SCROLL_LEFT_INDICATOR_STR.len());
                let reserved_scrollable =
                    SCROLL_LEFT_INDICATOR_STR.len() + SCROLL_RIGHT_INDICATOR_STR.len();
                if cols <= reserved_scrollable {
                    return Err(CalculatorEnvironmentError::new("Window too narrow").into());
                }
                let scroll_window_size = cols - reserved_scrollable;

                // Check if the cursor is still in scroll bounds. If it is not, change the scroll
                // bounds.
                if cursor_pos < scroll_offset || cursor_pos + 1 > scroll_offset + scroll_window_size
                {
                    if current_input.len() < scroll_window_size {
                        scroll_offset = 0;
                    } else {
                        let rel_cursor_pos =
                            max(1, min(scroll_window_size, (scroll_window_size / 3) * 2));
                        if cursor_pos < rel_cursor_pos {
                            scroll_offset = 0
                        } else {
                            scroll_offset = cursor_pos - rel_cursor_pos;
                        }
                    }
                }

                let opener_str = if scroll_offset == 0 {
                    PROMPT_STR
                } else {
                    SCROLL_LEFT_INDICATOR_STR
                };

                let overflow_right = current_input.len() > scroll_offset + scroll_window_size;
                let closer_str = if overflow_right {
                    SCROLL_RIGHT_INDICATOR_STR
                } else {
                    ""
                };
                let end_index = if overflow_right {
                    scroll_offset + scroll_window_size
                } else {
                    current_input.len()
                };
                let scrolled_cursor: u16 =
                    u16::try_from(cursor_pos - scroll_offset + opener_str.len())?;

                execute!(
                    stdout,
                    MoveToColumn(0),
                    Clear(CurrentLine),
                    Print(&opener_str),
                    Print(&current_input[scroll_offset..end_index]),
                    Print(&closer_str),
                    MoveToColumn(scrolled_cursor)
                )?;
            }

            if input_complete {
                break 'get_input_line;
            }

            // Loop until we match an event that we care about. Once we have one, we will use that
            // to change one of the values that determines what the terminal output looks like.
            // Then we will break out of this loop and go back to the top of the `'get_input_line`
            // loop to update the display. If the event indicates that we are quitting,  we will
            // instead break out of the `'calculate` loop. If the input line is done but we are not
            // quitting, we will set `input_complete` and break out of this loop, allowing us to
            // update the display one more time before exiting the `'get_input_line` loop.
            'get_event: loop {
                match event::read()? {
                    Event::Key(event) => match event.code {
                        KeyCode::Char(mut c) => {
                            if !c.is_ascii() {
                                continue 'get_event;
                            }
                            if event.modifiers == KeyModifiers::CONTROL {
                                if c == 'd' || c == 'z' || c == 'c' {
                                    // "Exit" commands.
                                    if !args.alternate_screen {
                                        // End this line before moving on.
                                        execute!(stdout, Print("\n"))?;
                                    }
                                    break 'calculate;
                                } else if c == 'm' {
                                    // "Find matching parenthesis" command.
                                    let current_input = inputs.current_line();
                                    if current_input.len() < 2 {
                                        continue 'get_event;
                                    }
                                    let mut pos = cursor_pos;
                                    if pos >= current_input.len() {
                                        pos = current_input.len() - 1;
                                    }
                                    let string_bytes = current_input.as_bytes();
                                    let (search_left, open_paren, close_paren) =
                                        match string_bytes[pos] {
                                            b'(' => (false, b'(', b')'),
                                            b')' => (true, b')', b'('),
                                            _ => continue 'get_event,
                                        };

                                    // We start `open_count` at `0`, but we also don't advance past
                                    // the starting parenthesis. So we will always increment it to
                                    // `1` at the beginning of the first loop. Then we will continue
                                    // to increment it when we see parentheses matching the one we
                                    // started on and decrement it when we see the opposite
                                    // parentheses. Once `open_count` is back down to `0`, we have
                                    // found the matching parenthesis.
                                    let mut open_count: usize = 0;
                                    loop {
                                        if string_bytes[pos] == open_paren {
                                            open_count += 1;
                                        } else if string_bytes[pos] == close_paren {
                                            open_count -= 1;
                                        }
                                        if open_count == 0 {
                                            cursor_pos = pos;
                                            break 'get_event;
                                        }
                                        // We hit the end of the string and never found the
                                        // corresponding parenthesis. Just give up and do nothing.
                                        if search_left && pos == 0 {
                                            continue 'get_event;
                                        } else if !search_left && pos + 1 >= string_bytes.len() {
                                            continue 'get_event;
                                        }
                                        if search_left {
                                            pos -= 1;
                                        } else {
                                            pos += 1;
                                        }
                                    }
                                }
                            }
                            if event.modifiers == KeyModifiers::SHIFT {
                                c = c.to_ascii_uppercase();
                            } else if !event.modifiers.is_empty() {
                                // This is a key combination that we don't handle. Just ignore the
                                // whole event.
                                continue 'get_event;
                            }
                            inputs.insert_char_into_current_line(cursor_pos, c);
                            cursor_pos += 1;
                            break 'get_event;
                        }
                        KeyCode::Backspace => {
                            if cursor_pos == 0 {
                                continue 'get_event;
                            }
                            cursor_pos -= 1;
                            inputs.remove_char_from_current_line(cursor_pos);
                            break 'get_event;
                        }
                        KeyCode::Delete => {
                            if cursor_pos >= inputs.current_line().len() {
                                continue 'get_event;
                            }
                            inputs.remove_char_from_current_line(cursor_pos);
                            break 'get_event;
                        }
                        KeyCode::Up => {
                            if !inputs.try_to_go_to_earlier_line(maybe_db.as_mut())? {
                                continue 'get_event;
                            }
                            cursor_pos = inputs.current_line().len();
                            scroll_offset = 0;
                            break 'get_event;
                        }
                        KeyCode::Down => {
                            if !inputs.try_to_go_to_later_line() {
                                continue 'get_event;
                            }
                            cursor_pos = inputs.current_line().len();
                            scroll_offset = 0;
                            break 'get_event;
                        }
                        KeyCode::Left => {
                            let distance: usize = if event.modifiers.is_empty() {
                                1
                            } else if event.modifiers == KeyModifiers::CONTROL {
                                LARGE_CURSOR_MOVE_DISTANCE
                            } else {
                                continue 'get_event;
                            };
                            if distance >= cursor_pos {
                                cursor_pos = 0;
                            } else {
                                cursor_pos -= distance;
                            }
                            break 'get_event;
                        }
                        KeyCode::Right => {
                            let distance: usize = if event.modifiers.is_empty() {
                                1
                            } else if event.modifiers == KeyModifiers::CONTROL {
                                LARGE_CURSOR_MOVE_DISTANCE
                            } else {
                                continue 'get_event;
                            };
                            let current_input_len = inputs.current_line().len();
                            if distance >= current_input_len
                                || cursor_pos >= current_input_len - distance
                            {
                                cursor_pos = current_input_len;
                            } else {
                                cursor_pos += distance;
                            }
                            break 'get_event;
                        }
                        KeyCode::Home => {
                            cursor_pos = 0;
                            break 'get_event;
                        }
                        KeyCode::End => {
                            cursor_pos = inputs.current_line().len();
                            break 'get_event;
                        }
                        KeyCode::Enter => {
                            input_complete = true;
                            break 'get_event;
                        }
                        _ => {}
                    },
                    Event::Paste(_) => {
                        // I want to implement this, but on my current system, pasting generates
                        // many key events, not a paste event. And I don't really want to implement
                        // something without being able to test it. So I'll leave this until I can
                        // find a way to actually get these events.
                        return Err(InternalCalculatorError::new("Paste unimplemented!").into());
                    }
                    Event::Resize(width, _) => {
                        cols = usize::from(width);
                        break 'get_event;
                    }
                    _ => {}
                } // match event::read()?
            } // 'get_event: loop
        } // 'get_input_line: loop

        let input = inputs.current_line().to_string();

        let output = match calculate(
            &input,
            args,
            &tokenizer,
            &mut command_executor,
            maybe_db.as_mut(),
            Some(&mut inputs),
            Some(&mut vars),
        ) {
            Ok(result) => result,
            // TODO: Display error position
            Err(CalculatorFailure::InputError(message)) => format!("Error: {}", message.value),
            Err(CalculatorFailure::RuntimeError(e)) => format!("Runtime Error: {}", e),
        };

        queue!(stdout, Print(output))?;
        if args.alternate_screen {
            queue!(stdout, MoveToNextLine(1))?;
        } else {
            // MoveToNextLine doesn't seem to always work properly if we aren't in the
            // alternate screen.
            queue!(stdout, Print("\n"))?;
        }
        stdout.flush()?;
    } // 'calculate: loop

    Ok(())
}

/// Evaluates the string input given to bcalc.
fn calculate(
    input: &str,
    args: &mut Args,
    tokenizer: &Tokenizer,
    command_executor: &mut CommandExecutor,
    mut maybe_db: Option<&mut SavedData>,
    mut maybe_inputs: Option<&mut InputHistory>,
    mut maybe_vars: Option<&mut VariableStore>,
) -> Result<String, CalculatorFailure> {
    let maybe_input_history_id = match maybe_inputs.as_mut() {
        Some(inputs) => inputs.input_finished(maybe_db.as_deref_mut())?,
        None => None,
    };

    let tokens = match tokenizer.tokenize(input, args.radix)? {
        ParsedInput::Tokens(t) => t,
        ParsedInput::Command((command, command_args)) => {
            let (message, vars_touched) = command_executor.execute_command(
                command,
                command_args,
                args,
                tokenizer,
                maybe_db.as_deref_mut(),
                maybe_inputs,
                maybe_vars.as_deref_mut(),
            )?;

            if let Some(vars) = maybe_vars {
                for var_name in vars_touched {
                    vars.touch(&var_name, maybe_input_history_id, maybe_db.as_deref_mut())?;
                }
            }

            return Ok(message);
        }
    };

    if let Some(vars) = maybe_vars.as_deref_mut() {
        let mut vars_touched: HashSet<String> = HashSet::new();
        for positioned_token in &tokens {
            match &positioned_token.value {
                Token::Variable(name) => {
                    vars_touched.insert(name.clone());
                }
                _ => {}
            }
        }
        for var_name in &vars_touched {
            vars.touch(&var_name, maybe_input_history_id, maybe_db.as_deref_mut())?;
        }
    }

    if tokens.is_empty() {
        return Ok(String::new());
    }

    let st = SyntaxTree::new(tokens.into())?;
    let result = st.execute(maybe_input_history_id, maybe_vars, maybe_db)?;

    // TODO: Allow displaying decimals better
    Ok(result.to_string())
}
