use crate::{
    position::{MaybePositioned, Positioned},
    token::{FunctionNameToken, Token},
};
use std::fmt;

#[derive(Debug)]
pub enum CalculatorFailure {
    /// Indicates an error that is the user's fault somehow (ex: invalid syntax, divided by 0,
    /// attempted to use a variable when the variable store is not available).
    InputError(MaybePositioned<String>),
    /// Indicates an error that is not the user's fault, such as failure to read the database.
    RuntimeError(Box<dyn std::error::Error>),
}

impl From<Positioned<String>> for CalculatorFailure {
    fn from(item: Positioned<String>) -> Self {
        CalculatorFailure::InputError(item.into())
    }
}

impl From<Box<dyn std::error::Error>> for CalculatorFailure {
    fn from(item: Box<dyn std::error::Error>) -> Self {
        CalculatorFailure::RuntimeError(item)
    }
}

#[derive(Debug)]
pub struct InternalCalculatorError {
    message: String,
}

impl InternalCalculatorError {
    pub fn new<S: Into<String>>(message: S) -> InternalCalculatorError {
        InternalCalculatorError {
            message: message.into(),
        }
    }
}

impl std::error::Error for InternalCalculatorError {}

impl fmt::Display for InternalCalculatorError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "InternalCalculatorError: {}", self.message)
    }
}

#[derive(Debug)]
pub struct CalculatorEnvironmentError {
    message: String,
}

impl CalculatorEnvironmentError {
    pub fn new<S: Into<String>>(message: S) -> CalculatorEnvironmentError {
        CalculatorEnvironmentError {
            message: message.into(),
        }
    }
}

impl std::error::Error for CalculatorEnvironmentError {}

impl fmt::Display for CalculatorEnvironmentError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "CalculatorEnvironmentError: {}", self.message)
    }
}

#[derive(Debug)]
pub struct CalculatorDatabaseInconsistencyError {
    message: String,
}

impl CalculatorDatabaseInconsistencyError {
    pub fn new<S: Into<String>>(message: S) -> CalculatorDatabaseInconsistencyError {
        CalculatorDatabaseInconsistencyError {
            message: message.into(),
        }
    }
}

impl std::error::Error for CalculatorDatabaseInconsistencyError {}

impl fmt::Display for CalculatorDatabaseInconsistencyError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "CalculatorDatabaseInconsistencyError: {}", self.message)
    }
}

#[derive(Clone, Debug)]
pub enum ParseError {
    NonAscii,
    InvalidNumber(String),
    InvalidVariable(String),
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ParseError::NonAscii => write!(f, "Non-ASCII data in input"),
            ParseError::InvalidNumber(s) => write!(f, "Unable to parse number: '{}'", s),
            ParseError::InvalidVariable(s) => write!(f, "Invalid variable name: '{}'", s),
        }
    }
}

impl From<Positioned<ParseError>> for CalculatorFailure {
    fn from(item: Positioned<ParseError>) -> Self {
        CalculatorFailure::InputError(item.map(|v| v.to_string()).into())
    }
}

#[derive(Clone, Debug)]
pub enum SyntaxError {
    NoInput,
    UnexpectedToken(Token),
    MismatchedOpenParen,
    MismatchedCloseParen,
    EmptyParens,
    MissingOperand(Token),
    CommaWithoutOperandBefore,
    CommaWithoutOperandAfter,
    FunctionWithoutParensOrArgument(FunctionNameToken),
    MissingOperator,
}

impl fmt::Display for SyntaxError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            SyntaxError::NoInput => write!(f, "No input"),
            SyntaxError::UnexpectedToken(token) => {
                write!(f, "Unexpected token encountered: {}", token)
            }
            SyntaxError::MismatchedOpenParen => write!(f, "Mismatched open parenthesis"),
            SyntaxError::MismatchedCloseParen => write!(f, "Mismatched close parenthesis"),
            SyntaxError::EmptyParens => write!(f, "Empty parentheses"),
            SyntaxError::MissingOperand(token) => {
                write!(f, "{} is missing a required operand", token)
            }
            SyntaxError::CommaWithoutOperandBefore => write!(f, "Comma must follow an operand"),
            SyntaxError::CommaWithoutOperandAfter => {
                write!(f, "Comma must be followed by an operand")
            }
            SyntaxError::FunctionWithoutParensOrArgument(function) => {
                write!(
                    f,
                    concat!(
                        "Functions without parentheses are assumed to have a single argument, but ",
                        "none was found for {}"
                    ),
                    function
                )
            }
            SyntaxError::MissingOperator => {
                write!(f, "Missing an operator between two consecutive operands")
            }
        }
    }
}

impl From<Positioned<SyntaxError>> for CalculatorFailure {
    fn from(item: Positioned<SyntaxError>) -> Self {
        CalculatorFailure::InputError(item.map(|v| v.to_string()).into())
    }
}

#[derive(Clone, Debug)]
pub enum MathExecutionError {
    UnknownVariable(String),
    DivisionByZero,
    FunctionNeedsArguments(FunctionNameToken),
    Unimplemented,
}

impl fmt::Display for MathExecutionError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            MathExecutionError::UnknownVariable(name) => write!(f, "Unknown variable: {}", name),
            MathExecutionError::DivisionByZero => write!(f, "Cannot divide by 0"),
            MathExecutionError::FunctionNeedsArguments(function) => {
                write!(f, "{} has no arguments but requires them", function)
            }
            MathExecutionError::Unimplemented => {
                write!(f, "Encountered operation that is not yet supported")
            }
        }
    }
}

impl From<Positioned<MathExecutionError>> for CalculatorFailure {
    fn from(item: Positioned<MathExecutionError>) -> Self {
        CalculatorFailure::InputError(item.map(|v| v.to_string()).into())
    }
}

#[derive(Clone, Debug)]
pub enum MissingCapabilityError {
    NoVariableStore,
    NoDatabase,
}

impl fmt::Display for MissingCapabilityError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            MissingCapabilityError::NoVariableStore => write!(f, "Variable store unavailable"),
            MissingCapabilityError::NoDatabase => write!(f, "Database unavailable"),
        }
    }
}

impl From<MissingCapabilityError> for CalculatorFailure {
    fn from(item: MissingCapabilityError) -> Self {
        CalculatorFailure::InputError(MaybePositioned::new_unpositioned(item.to_string()))
    }
}

impl From<Positioned<MissingCapabilityError>> for CalculatorFailure {
    fn from(item: Positioned<MissingCapabilityError>) -> Self {
        CalculatorFailure::InputError(item.map(|v| v.to_string()).into())
    }
}
