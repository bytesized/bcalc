use crate::{
    error::ParseError,
    position::{Position, Positioned},
};
use num::{bigint::BigInt, pow::Pow, rational::BigRational};
use std::{collections::HashMap, fmt};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnaryOperatorToken {
    SquareRoot,
    Negate,
    AbsoluteValue,
}

impl fmt::Display for UnaryOperatorToken {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            UnaryOperatorToken::SquareRoot => write!(f, "Square Root Operator (sqrt)"),
            UnaryOperatorToken::Negate => write!(f, "Negation Operator (-)"),
            UnaryOperatorToken::AbsoluteValue => write!(f, "Absolute Value Operator (abs)"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinaryOperatorToken {
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulus,
    Exponent,
}

// TODO: Is there some way to check, ideally at compile time, that every variant of
// `BinaryOperatorToken` is in `ORDERED_BINARY_OPERATORS`?
pub const ORDERED_BINARY_OPERATORS: &'static [&'static [BinaryOperatorToken]] = &[
    &[BinaryOperatorToken::Exponent],
    &[BinaryOperatorToken::Modulus],
    &[BinaryOperatorToken::Multiply, BinaryOperatorToken::Divide],
    &[BinaryOperatorToken::Add, BinaryOperatorToken::Subtract],
];

impl fmt::Display for BinaryOperatorToken {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            BinaryOperatorToken::Add => write!(f, "Addition Operator (+)"),
            BinaryOperatorToken::Subtract => write!(f, "Subtraction Operator (-)"),
            BinaryOperatorToken::Multiply => write!(f, "Multiplication Operator (*)"),
            BinaryOperatorToken::Divide => write!(f, "Division Operator (/)"),
            BinaryOperatorToken::Modulus => write!(f, "Modulus Operator (%)"),
            BinaryOperatorToken::Exponent => write!(f, "Exponentiation Operator (^)"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FunctionNameToken {
    Max,
    Min,
}

impl fmt::Display for FunctionNameToken {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            FunctionNameToken::Max => write!(f, "Max Function"),
            FunctionNameToken::Min => write!(f, "Min Function"),
        }
    }
}

#[derive(Clone, Debug)]
pub enum Token {
    Variable(String),
    AssignmentOperator,
    Comma,
    Number(BigRational),
    OpenParen,
    CloseParen,
    BinaryOperator(BinaryOperatorToken),
    UnaryOperator(UnaryOperatorToken),
    Function(FunctionNameToken),
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Token::Variable(s) => write!(f, "Variable '{}'", s),
            Token::AssignmentOperator => write!(f, "Assignment Operator (=)"),
            Token::Comma => write!(f, "Comma"),
            Token::Number(n) => write!(f, "Number ({})", n),
            Token::OpenParen => write!(f, "Open Parenthesis"),
            Token::CloseParen => write!(f, "Close Parenthesis"),
            Token::BinaryOperator(t) => fmt::Display::fmt(t, f),
            Token::UnaryOperator(t) => fmt::Display::fmt(t, f),
            Token::Function(t) => fmt::Display::fmt(t, f),
        }
    }
}

impl From<BigRational> for Token {
    fn from(item: BigRational) -> Self {
        Token::Number(item)
    }
}

impl From<BinaryOperatorToken> for Token {
    fn from(item: BinaryOperatorToken) -> Self {
        Token::BinaryOperator(item)
    }
}

impl From<UnaryOperatorToken> for Token {
    fn from(item: UnaryOperatorToken) -> Self {
        Token::UnaryOperator(item)
    }
}

impl From<FunctionNameToken> for Token {
    fn from(item: FunctionNameToken) -> Self {
        Token::Function(item)
    }
}

#[derive(Clone, Debug)]
pub enum ParsedInput {
    Tokens(Vec<Positioned<Token>>),
    // Commands are very lightly processed to allow them to be as flexible as possible. They are
    // simply split into the command name in one string and all the arguments in another string.
    Command((Positioned<String>, Positioned<String>)),
}

pub struct Tokenizer {
    token_map: HashMap<String, Token>,
}

impl Tokenizer {
    pub fn new() -> Tokenizer {
        let mut token_map: HashMap<String, Token> = HashMap::new();
        token_map.insert("sqrt".to_string(), UnaryOperatorToken::SquareRoot.into());
        token_map.insert("abs".to_string(), UnaryOperatorToken::AbsoluteValue.into());
        token_map.insert("max".to_string(), FunctionNameToken::Max.into());
        token_map.insert("min".to_string(), FunctionNameToken::Min.into());

        Tokenizer { token_map }
    }

    /// Takes a string of input. Returns a vector of tokens.
    /// Does not validate that the tokens make sense in the given order.
    /// Interprets all `-` characters as `BinaryOperatorToken::Subtract`, even if they logically
    /// make more sense as `UnaryOperatorToken::Negate`. At the token parsing stage there isn't
    /// really a good way to tell them apart. We'll correct this later when we generate the syntax
    /// tree for the tokens.
    pub fn tokenize(&self, input: &str, radix: u8) -> Result<ParsedInput, Positioned<ParseError>> {
        let mut tokens: Vec<Positioned<Token>> = Vec::new();
        // When we are in the middle of a multi-character token (i.e. a number or a variable), we
        // will store it in this buffer until we are at the end (whitespace or a single-character
        // token) and then we turn the contents of buffer into a token.
        let mut buffer: Vec<u8> = Vec::new();

        for (position, chr) in input.chars().enumerate() {
            if !chr.is_ascii() {
                return Err(Positioned::new_raw(ParseError::NonAscii, position, 1));
            }
        }

        if let Some(command) = self.maybe_extract_command(input)? {
            return Ok(command);
        }

        let input = input.as_bytes();

        for (position, chr) in input.iter().enumerate() {
            if (*chr as char).is_ascii_whitespace() {
                self.tokenize_on_multichar_end(&mut tokens, &mut buffer, position, radix)?;
            } else {
                let maybe_token: Option<Token> = match chr {
                    b'+' => Some(BinaryOperatorToken::Add.into()),
                    b'-' => Some(BinaryOperatorToken::Subtract.into()),
                    b'*' => Some(BinaryOperatorToken::Multiply.into()),
                    b'/' => Some(BinaryOperatorToken::Divide.into()),
                    b'%' => Some(BinaryOperatorToken::Modulus.into()),
                    b'^' => Some(BinaryOperatorToken::Exponent.into()),
                    b'(' => Some(Token::OpenParen),
                    b')' => Some(Token::CloseParen),
                    b'=' => Some(Token::AssignmentOperator),
                    b',' => Some(Token::Comma),
                    _ => None,
                };

                match maybe_token {
                    Some(token) => {
                        self.tokenize_on_multichar_end(&mut tokens, &mut buffer, position, radix)?;
                        tokens.push(Positioned::new_raw(token, position, 1));
                    }
                    None => {
                        buffer.push(*chr);
                    }
                }
            }
        }

        self.tokenize_on_multichar_end(&mut tokens, &mut buffer, input.len(), radix)?;

        Ok(ParsedInput::Tokens(tokens))
    }

    fn maybe_extract_command(
        &self,
        input: &str,
    ) -> Result<Option<ParsedInput>, Positioned<ParseError>> {
        let trimmed_input = input.trim_start();
        let command_start = input.len() - trimmed_input.len();

        let post_slash = match trimmed_input.strip_prefix('/') {
            None => return Ok(None),
            Some(suffix) => suffix,
        };
        let (command, args) = match post_slash.split_once(|c: char| c.is_ascii_whitespace()) {
            None => {
                let command =
                    Positioned::new_raw(post_slash.to_string(), command_start, trimmed_input.len());
                let args =
                    Positioned::new_raw(String::new(), command_start + trimmed_input.len(), 0);
                (command, args)
            }
            Some((prefix, suffix)) => {
                // Presumably `1`, but just to be thorough...
                let slash_len = trimmed_input.len() - post_slash.len();
                let command = Positioned::new_raw(
                    prefix.to_string(),
                    command_start,
                    slash_len + prefix.len(),
                );
                let args = Positioned::new_raw(
                    suffix.to_string(),
                    input.len() - suffix.len(),
                    suffix.len(),
                );
                (command, args)
            }
        };
        Ok(Some(ParsedInput::Command((command, args))))
    }

    // Helper function for `tokenize`. When we get to the boundary between tokens (whitespace,
    // single character operators, or the end of input), we will call this function to interpret
    // what we have read and, assuming that anything is in the buffer, turn it into some sort of
    // token and add it to the `tokens` vector.
    // `buffer` will always be empty after this function returns `Ok`.
    fn tokenize_on_multichar_end(
        &self,
        tokens: &mut Vec<Positioned<Token>>,
        buffer: &mut Vec<u8>,
        buffer_stop_position: usize,
        radix: u8,
    ) -> Result<(), Positioned<ParseError>> {
        if buffer.is_empty() {
            return Ok(());
        }

        let width = buffer.len();
        let buffer_start = buffer_stop_position - width;
        // Since `buffer` only contains ASCII, this is safe.
        let buffer_as_string = String::from_utf8(buffer.clone()).unwrap();

        if buffer[0] == b'$' {
            tokens.push(Positioned::new_raw(
                Token::Variable(buffer_as_string),
                buffer_start,
                width,
            ));
            buffer.clear();
            return Ok(());
        }

        if let Some(token) = self.token_map.get(&buffer_as_string) {
            tokens.push(Positioned::new_raw(token.clone(), buffer_start, width));
            buffer.clear();
            return Ok(());
        }

        // We've exhausted the other options. The fall through case is that this is a number.
        // To parse it, we first need to pull out any '_' characters (which we allow as arbitrary
        // separators) and, if there is a decimal point, we need to pull it out and note its
        // position.
        let mut clean_buffer: Vec<u8> = Vec::new();
        let mut maybe_dec_index: Option<usize> = None;
        for chr in buffer.iter() {
            if *chr == b'_' {
                continue;
            } else if *chr == b'.' && maybe_dec_index.is_none() {
                // We specifically only pull out the first decimal point  found. Finding more than
                // one should generate an error, which is just what will happen below if we give a
                // buffer with a decimal to `BigInt::parse_bytes`.
                maybe_dec_index = Some(clean_buffer.len());
                continue;
            }
            clean_buffer.push(*chr);
        }

        let numer = BigInt::parse_bytes(&clean_buffer, radix.into()).ok_or_else(|| {
            Positioned::new_raw(
                ParseError::InvalidNumber(buffer_as_string),
                buffer_start,
                width,
            )
        })?;

        let denom = match maybe_dec_index {
            Some(dec_index) => {
                let big_radix = BigInt::from(radix);
                big_radix.pow(clean_buffer.len() - dec_index)
            }
            None => BigInt::from(1),
        };

        tokens.push(Positioned::new_raw(
            Token::Number(BigRational::new(numer, denom)),
            buffer_start,
            width,
        ));

        buffer.clear();
        Ok(())
    }

    pub fn tokenize_variable_list(
        &self,
        input: &str,
    ) -> Result<Vec<Positioned<String>>, Positioned<String>> {
        // `radix` should be meaningless here, but passing in 10 makes the error messages a bit
        // easier to deal with.
        let positioned_tokens = match self.tokenize(input, 10) {
            Err(positioned_error) => {
                let message = match positioned_error.value {
                    ParseError::InvalidVariable(s) | ParseError::InvalidNumber(s) => {
                        ParseError::InvalidVariable(s).to_string()
                    }
                    ParseError::NonAscii => ParseError::NonAscii.to_string(),
                };
                return Err(Positioned::new(message, positioned_error.position));
            }
            Ok(ParsedInput::Command((command_name, _))) => {
                return Err(Positioned::new(
                    ParseError::InvalidVariable(format!("/{}", command_name.value)).to_string(),
                    command_name.position,
                ))
            }
            Ok(ParsedInput::Tokens(t)) => t,
        };

        let mut result: Vec<Positioned<String>> = Vec::new();
        for positioned_token in positioned_tokens {
            match positioned_token.value {
                Token::Variable(s) => result.push(Positioned::new(s, positioned_token.position)),
                token => {
                    return Err(Positioned::new(
                        format!("Expected variable, found {}", token),
                        positioned_token.position,
                    ))
                }
            }
        }

        Ok(result)
    }

    pub fn tokenize_int_list(
        &self,
        input: &str,
        radix: u8,
    ) -> Result<Vec<Positioned<i64>>, Positioned<String>> {
        let positioned_tokens = match self.tokenize(input, radix) {
            Err(positioned_error) => {
                let message = match positioned_error.value {
                    ParseError::InvalidVariable(s) | ParseError::InvalidNumber(s) => {
                        ParseError::InvalidVariable(s).to_string()
                    }
                    ParseError::NonAscii => ParseError::NonAscii.to_string(),
                };
                return Err(Positioned::new(message, positioned_error.position));
            }
            Ok(ParsedInput::Command((command_name, _))) => {
                return Err(Positioned::new(
                    ParseError::InvalidNumber(format!("/{}", command_name.value)).to_string(),
                    command_name.position,
                ))
            }
            Ok(ParsedInput::Tokens(t)) => t,
        };

        let mut result: Vec<Positioned<i64>> = Vec::new();
        let mut just_read_negative_sign_pos: Option<Position> = None;
        for positioned_token in positioned_tokens {
            match positioned_token.value {
                Token::Number(number) => {
                    if number.denom() != &BigInt::from(1) {
                        // TODO: display decimal better
                        return Err(Positioned::new(
                            format!("Expected an integer, found decimal: {}", number),
                            positioned_token.position,
                        ));
                    }
                    match i64::try_from(number.numer()) {
                        Ok(integer) => match just_read_negative_sign_pos {
                            Some(neg_pos) => {
                                result.push(Positioned::new_span(
                                    -integer,
                                    neg_pos,
                                    positioned_token.position,
                                ));
                                just_read_negative_sign_pos = None;
                            }
                            None => {
                                result.push(Positioned::new(integer, positioned_token.position));
                            }
                        },
                        Err(_) => {
                            return Err(Positioned::new(
                                "Value must be representable as a 64-bit signed integer"
                                    .to_string(),
                                positioned_token.position,
                            ));
                        }
                    }
                }
                Token::BinaryOperator(BinaryOperatorToken::Subtract) => {
                    if let Some(prev_position) = just_read_negative_sign_pos {
                        return Err(Positioned::new_span(
                            "Two negative signs in a row".to_string(),
                            prev_position,
                            positioned_token.position,
                        ));
                    }
                    just_read_negative_sign_pos = Some(positioned_token.position);
                }
                token => {
                    return Err(Positioned::new(
                        format!("Expected integer, found {}", token),
                        positioned_token.position,
                    ));
                }
            }
        }

        if let Some(neg_pos) = just_read_negative_sign_pos {
            return Err(Positioned::new(
                "Found negative sign without value".to_string(),
                neg_pos,
            ));
        }

        Ok(result)
    }
}

#[cfg(test)]
mod token_parsing_tests {
    use crate::{
        error::ParseError,
        position::Positioned,
        token::{
            BinaryOperatorToken, FunctionNameToken, ParsedInput, Token, Tokenizer,
            UnaryOperatorToken,
        },
    };
    use num::bigint::BigInt;

    fn get_tokens(input: &str, radix: u8) -> Vec<Positioned<Token>> {
        let tokenizer = Tokenizer::new();
        let parsed = tokenizer.tokenize(input, radix).unwrap();
        match parsed {
            ParsedInput::Tokens(t) => t,
            _ => panic!(),
        }
    }

    fn assert_variable(token: Positioned<Token>, name: &str, start: usize, width: usize) {
        assert_eq!(token.position.start, start);
        assert_eq!(token.position.width, width);
        match token.value {
            Token::Variable(n) => assert_eq!(n, name),
            _ => panic!(),
        }
    }

    fn assert_assignment(token: Positioned<Token>, start: usize, width: usize) {
        assert_eq!(token.position.start, start);
        assert_eq!(token.position.width, width);
        match token.value {
            Token::AssignmentOperator => {}
            _ => panic!(),
        }
    }

    fn assert_number(token: Positioned<Token>, numer: u64, denom: u64, start: usize, width: usize) {
        assert_eq!(token.position.start, start);
        assert_eq!(token.position.width, width);
        match token.value {
            Token::Number(n) => {
                assert_eq!(n.numer(), &BigInt::from(numer));
                assert_eq!(n.denom(), &BigInt::from(denom));
            }
            _ => panic!(),
        }
    }

    fn assert_comma(token: Positioned<Token>, start: usize, width: usize) {
        assert_eq!(token.position.start, start);
        assert_eq!(token.position.width, width);
        match token.value {
            Token::Comma => {}
            _ => panic!(),
        }
    }

    fn assert_open_paren(token: Positioned<Token>, start: usize, width: usize) {
        assert_eq!(token.position.start, start);
        assert_eq!(token.position.width, width);
        match token.value {
            Token::OpenParen => {}
            _ => panic!(),
        }
    }

    fn assert_close_paren(token: Positioned<Token>, start: usize, width: usize) {
        assert_eq!(token.position.start, start);
        assert_eq!(token.position.width, width);
        match token.value {
            Token::CloseParen => {}
            _ => panic!(),
        }
    }

    fn assert_add_op(token: Positioned<Token>, start: usize, width: usize) {
        assert_eq!(token.position.start, start);
        assert_eq!(token.position.width, width);
        match token.value {
            Token::BinaryOperator(BinaryOperatorToken::Add) => {}
            _ => panic!(),
        }
    }

    fn assert_subtract_op(token: Positioned<Token>, start: usize, width: usize) {
        assert_eq!(token.position.start, start);
        assert_eq!(token.position.width, width);
        match token.value {
            Token::BinaryOperator(BinaryOperatorToken::Subtract) => {}
            _ => panic!(),
        }
    }

    fn assert_multiply_op(token: Positioned<Token>, start: usize, width: usize) {
        assert_eq!(token.position.start, start);
        assert_eq!(token.position.width, width);
        match token.value {
            Token::BinaryOperator(BinaryOperatorToken::Multiply) => {}
            _ => panic!(),
        }
    }

    fn assert_divide_op(token: Positioned<Token>, start: usize, width: usize) {
        assert_eq!(token.position.start, start);
        assert_eq!(token.position.width, width);
        match token.value {
            Token::BinaryOperator(BinaryOperatorToken::Divide) => {}
            _ => panic!(),
        }
    }

    fn assert_modulus_op(token: Positioned<Token>, start: usize, width: usize) {
        assert_eq!(token.position.start, start);
        assert_eq!(token.position.width, width);
        match token.value {
            Token::BinaryOperator(BinaryOperatorToken::Modulus) => {}
            _ => panic!(),
        }
    }

    fn assert_exponent_op(token: Positioned<Token>, start: usize, width: usize) {
        assert_eq!(token.position.start, start);
        assert_eq!(token.position.width, width);
        match token.value {
            Token::BinaryOperator(BinaryOperatorToken::Exponent) => {}
            _ => panic!(),
        }
    }

    fn assert_sqrt_op(token: Positioned<Token>, start: usize, width: usize) {
        assert_eq!(token.position.start, start);
        assert_eq!(token.position.width, width);
        match token.value {
            Token::UnaryOperator(UnaryOperatorToken::SquareRoot) => {}
            _ => panic!(),
        }
    }

    fn assert_abs_op(token: Positioned<Token>, start: usize, width: usize) {
        assert_eq!(token.position.start, start);
        assert_eq!(token.position.width, width);
        match token.value {
            Token::UnaryOperator(UnaryOperatorToken::AbsoluteValue) => {}
            _ => panic!(),
        }
    }

    fn assert_max_fn(token: Positioned<Token>, start: usize, width: usize) {
        assert_eq!(token.position.start, start);
        assert_eq!(token.position.width, width);
        match token.value {
            Token::Function(FunctionNameToken::Max) => {}
            _ => panic!(),
        }
    }

    fn assert_min_fn(token: Positioned<Token>, start: usize, width: usize) {
        assert_eq!(token.position.start, start);
        assert_eq!(token.position.width, width);
        match token.value {
            Token::Function(FunctionNameToken::Min) => {}
            _ => panic!(),
        }
    }

    #[test]
    fn all_tokens_no_spaces() {
        let tokens = get_tokens("$var=1,.1()+-*/%^sqrt,abs,max,min", 10);
        let mut token_iter = tokens.into_iter();
        assert_variable(token_iter.next().unwrap(), "$var", 0, 4);
        assert_assignment(token_iter.next().unwrap(), 4, 1);
        assert_number(token_iter.next().unwrap(), 1, 1, 5, 1);
        assert_comma(token_iter.next().unwrap(), 6, 1);
        assert_number(token_iter.next().unwrap(), 1, 10, 7, 2);
        assert_open_paren(token_iter.next().unwrap(), 9, 1);
        assert_close_paren(token_iter.next().unwrap(), 10, 1);
        assert_add_op(token_iter.next().unwrap(), 11, 1);
        assert_subtract_op(token_iter.next().unwrap(), 12, 1);
        assert_multiply_op(token_iter.next().unwrap(), 13, 1);
        assert_divide_op(token_iter.next().unwrap(), 14, 1);
        assert_modulus_op(token_iter.next().unwrap(), 15, 1);
        assert_exponent_op(token_iter.next().unwrap(), 16, 1);
        assert_sqrt_op(token_iter.next().unwrap(), 17, 4);
        assert_comma(token_iter.next().unwrap(), 21, 1);
        assert_abs_op(token_iter.next().unwrap(), 22, 3);
        assert_comma(token_iter.next().unwrap(), 25, 1);
        assert_max_fn(token_iter.next().unwrap(), 26, 3);
        assert_comma(token_iter.next().unwrap(), 29, 1);
        assert_min_fn(token_iter.next().unwrap(), 30, 3);
        assert!(token_iter.next().is_none());
    }

    #[test]
    fn all_tokens_with_spaces() {
        let tokens = get_tokens(" $var = , 1 1.1 ( ) + - * / % ^ sqrt abs max min ", 10);
        let mut token_iter = tokens.into_iter();
        assert_variable(token_iter.next().unwrap(), "$var", 1, 4);
        assert_assignment(token_iter.next().unwrap(), 6, 1);
        assert_comma(token_iter.next().unwrap(), 8, 1);
        assert_number(token_iter.next().unwrap(), 1, 1, 10, 1);
        assert_number(token_iter.next().unwrap(), 11, 10, 12, 3);
        assert_open_paren(token_iter.next().unwrap(), 16, 1);
        assert_close_paren(token_iter.next().unwrap(), 18, 1);
        assert_add_op(token_iter.next().unwrap(), 20, 1);
        assert_subtract_op(token_iter.next().unwrap(), 22, 1);
        assert_multiply_op(token_iter.next().unwrap(), 24, 1);
        assert_divide_op(token_iter.next().unwrap(), 26, 1);
        assert_modulus_op(token_iter.next().unwrap(), 28, 1);
        assert_exponent_op(token_iter.next().unwrap(), 30, 1);
        assert_sqrt_op(token_iter.next().unwrap(), 32, 4);
        assert_abs_op(token_iter.next().unwrap(), 37, 3);
        assert_max_fn(token_iter.next().unwrap(), 41, 3);
        assert_min_fn(token_iter.next().unwrap(), 45, 3);
        assert!(token_iter.next().is_none());
    }

    #[test]
    fn multiple_decimal_points() {
        let tokenizer = Tokenizer::new();
        let error = tokenizer.tokenize("1.1.1", 10).unwrap_err();
        match error.value {
            ParseError::InvalidNumber(_) => {}
            _ => panic!(),
        }
        assert_eq!(error.position.start, 0);
        assert_eq!(error.position.width, 5);
    }

    #[test]
    fn hexadecimal_upper() {
        let tokens = get_tokens("0123456789ABCDEF", 16);
        let mut token_iter = tokens.into_iter();
        assert_number(token_iter.next().unwrap(), 81985529216486895, 1, 0, 16);
        assert!(token_iter.next().is_none());
    }

    #[test]
    fn hexadecimal_lower() {
        let tokens = get_tokens("0123456789abcdef", 16);
        let mut token_iter = tokens.into_iter();
        assert_number(token_iter.next().unwrap(), 81985529216486895, 1, 0, 16);
        assert!(token_iter.next().is_none());
    }

    #[test]
    fn out_of_radix_range() {
        let tokenizer = Tokenizer::new();
        let error = tokenizer.tokenize("9", 8).unwrap_err();
        match error.value {
            ParseError::InvalidNumber(_) => {}
            _ => panic!(),
        }
        assert_eq!(error.position.start, 0);
        assert_eq!(error.position.width, 1);
    }

    fn get_command(input: &str) -> (Positioned<String>, Positioned<String>) {
        let tokenizer = Tokenizer::new();
        let parsed = tokenizer.tokenize(input, 10).unwrap();
        match parsed {
            ParsedInput::Command((c, a)) => (c, a),
            _ => panic!(),
        }
    }

    fn assert_pos_string(input: Positioned<String>, value: &str, start: usize, width: usize) {
        assert_eq!(input.value, value);
        assert_eq!(input.position.start, start);
        assert_eq!(input.position.width, width);
    }

    #[test]
    fn bare_command() {
        let (command, args) = get_command("/command");
        assert_pos_string(command, "command", 0, 8);
        assert_pos_string(args, "", 8, 0);
    }

    #[test]
    fn command_with_arg() {
        let (command, args) = get_command("/command arg1 arg2");
        assert_pos_string(command, "command", 0, 8);
        assert_pos_string(args, "arg1 arg2", 9, 9);
    }

    #[test]
    fn command_whitespace_arg() {
        let (command, args) = get_command("  /command     ");
        assert_pos_string(command, "command", 2, 8);
        assert_pos_string(args, "    ", 11, 4);
    }

    #[test]
    fn empty_var_list() {
        let tokenizer = Tokenizer::new();
        let vars = tokenizer.tokenize_variable_list("").unwrap();
        assert!(vars.is_empty());
    }

    #[test]
    fn singleton_var_list() {
        let tokenizer = Tokenizer::new();
        let vars = tokenizer.tokenize_variable_list("$var").unwrap();
        let mut vars_iter = vars.into_iter();
        assert_pos_string(vars_iter.next().unwrap(), "$var", 0, 4);
        assert!(vars_iter.next().is_none());
    }

    #[test]
    fn multiple_var_list() {
        let tokenizer = Tokenizer::new();
        let vars = tokenizer
            .tokenize_variable_list("$var1 $var2 $var3")
            .unwrap();
        let mut var_iter = vars.into_iter();
        assert_pos_string(var_iter.next().unwrap(), "$var1", 0, 5);
        assert_pos_string(var_iter.next().unwrap(), "$var2", 6, 5);
        assert_pos_string(var_iter.next().unwrap(), "$var3", 12, 5);
        assert!(var_iter.next().is_none());
    }

    fn assert_int(input: Positioned<i64>, value: i64, start: usize, width: usize) {
        assert_eq!(input.value, value);
        assert_eq!(input.position.start, start);
        assert_eq!(input.position.width, width);
    }

    #[test]
    fn empty_int_list() {
        let tokenizer = Tokenizer::new();
        let ints = tokenizer.tokenize_int_list("", 10).unwrap();
        assert!(ints.is_empty());
    }

    #[test]
    fn singleton_int_list() {
        let tokenizer = Tokenizer::new();
        let ints = tokenizer.tokenize_int_list("123", 10).unwrap();
        let mut int_iter = ints.into_iter();
        assert_int(int_iter.next().unwrap(), 123, 0, 3);
        assert!(int_iter.next().is_none());
    }

    #[test]
    fn singleton_negative_int_list() {
        let tokenizer = Tokenizer::new();
        let ints = tokenizer.tokenize_int_list("-123", 10).unwrap();
        let mut int_iter = ints.into_iter();
        assert_int(int_iter.next().unwrap(), -123, 0, 4);
        assert!(int_iter.next().is_none());
    }

    #[test]
    fn multiple_int_list() {
        let tokenizer = Tokenizer::new();
        let ints = tokenizer.tokenize_int_list("123 456 789", 10).unwrap();
        let mut int_iter = ints.into_iter();
        assert_int(int_iter.next().unwrap(), 123, 0, 3);
        assert_int(int_iter.next().unwrap(), 456, 4, 3);
        assert_int(int_iter.next().unwrap(), 789, 8, 3);
        assert!(int_iter.next().is_none());
    }

    #[test]
    fn multiple_negative_int_list() {
        let tokenizer = Tokenizer::new();
        let ints = tokenizer.tokenize_int_list("-123 456 -789", 10).unwrap();
        let mut int_iter = ints.into_iter();
        assert_int(int_iter.next().unwrap(), -123, 0, 4);
        assert_int(int_iter.next().unwrap(), 456, 5, 3);
        assert_int(int_iter.next().unwrap(), -789, 9, 4);
        assert!(int_iter.next().is_none());
    }
}
