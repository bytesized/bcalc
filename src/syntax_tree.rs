use crate::{
    error::{
        CalculatorFailure,
        MathExecutionError::{
            DivisionByZero, FunctionNeedsArguments, Unimplemented, UnknownVariable,
        },
        MissingCapabilityError::NoVariableStore,
        SyntaxError::{
            self, CommaWithoutOperandAfter, CommaWithoutOperandBefore, EmptyParens,
            FunctionWithoutParensOrArgument, MismatchedCloseParen, MismatchedOpenParen,
            MissingOperand, MissingOperator, NoInput, UnexpectedToken,
        },
    },
    position::{Position, Positioned},
    saved_data::SavedData,
    token::{
        BinaryOperatorToken, FunctionNameToken, Token, UnaryOperatorToken, ORDERED_BINARY_OPERATORS,
    },
    variable::{Variable, VariableStore},
};
use num::{bigint::BigInt, pow::Pow, rational::BigRational, Signed};
use std::{
    cmp::{max, min},
    collections::VecDeque,
    mem,
};

trait OperationNode {
    fn execute(
        self: Box<Self>,
        maybe_vars: Option<&mut VariableStore>,
        maybe_db: Option<&mut SavedData>,
    ) -> Result<BigRational, CalculatorFailure>;

    fn position(&self) -> Position;
}

#[derive(Clone, Debug)]
struct NumericNode {
    value: BigRational,
    position: Position,
}

impl OperationNode for NumericNode {
    fn execute(
        self: Box<Self>,
        _maybe_vars: Option<&mut VariableStore>,
        _maybe_db: Option<&mut SavedData>,
    ) -> Result<BigRational, CalculatorFailure> {
        Ok(self.value)
    }

    fn position(&self) -> Position {
        self.position.clone()
    }
}

#[derive(Clone, Debug)]
struct VariableNode {
    name: String,
    position: Position,
}

impl OperationNode for VariableNode {
    fn execute(
        self: Box<Self>,
        maybe_vars: Option<&mut VariableStore>,
        maybe_db: Option<&mut SavedData>,
    ) -> Result<BigRational, CalculatorFailure> {
        let vars = match maybe_vars {
            Some(v) => v,
            None => return Err(Positioned::new(NoVariableStore, self.position).into()),
        };
        let variable = vars
            .get(self.name.clone(), maybe_db)?
            .ok_or_else(|| Positioned::new(UnknownVariable(self.name), self.position))?;
        Ok(variable.value)
    }

    fn position(&self) -> Position {
        self.position.clone()
    }
}

#[derive(Clone, Debug)]
struct UnaryNode {
    operator: UnaryOperatorToken,
    operator_position: Position,
    operand: SyntaxTreeNode,
}

impl OperationNode for UnaryNode {
    fn execute(
        self: Box<Self>,
        mut maybe_vars: Option<&mut VariableStore>,
        mut maybe_db: Option<&mut SavedData>,
    ) -> Result<BigRational, CalculatorFailure> {
        let operand = self
            .operand
            .execute(maybe_vars.as_deref_mut(), maybe_db.as_deref_mut())?;
        match self.operator {
            UnaryOperatorToken::SquareRoot => {
                // TODO: Implement
                return Err(Positioned::new(Unimplemented, self.operator_position).into());
            }
            UnaryOperatorToken::Negate => Ok(-operand),
            UnaryOperatorToken::AbsoluteValue => Ok(operand.abs()),
        }
    }

    fn position(&self) -> Position {
        Position::from_span(self.operator_position.clone(), self.operand.position())
    }
}

#[derive(Clone, Debug)]
struct BinaryNode {
    operator: BinaryOperatorToken,
    operator_position: Position,
    operand_1: SyntaxTreeNode,
    operand_2: SyntaxTreeNode,
}

impl OperationNode for BinaryNode {
    fn execute(
        self: Box<Self>,
        mut maybe_vars: Option<&mut VariableStore>,
        mut maybe_db: Option<&mut SavedData>,
    ) -> Result<BigRational, CalculatorFailure> {
        let operand_1 = self
            .operand_1
            .execute(maybe_vars.as_deref_mut(), maybe_db.as_deref_mut())?;
        let operand_2 = self
            .operand_2
            .execute(maybe_vars.as_deref_mut(), maybe_db.as_deref_mut())?;
        match self.operator {
            BinaryOperatorToken::Add => Ok(operand_1 + operand_2),
            BinaryOperatorToken::Subtract => Ok(operand_1 - operand_2),
            BinaryOperatorToken::Multiply => Ok(operand_1 * operand_2),
            BinaryOperatorToken::Divide => {
                if *operand_2.numer() == BigInt::from(0) {
                    return Err(Positioned::new(DivisionByZero, self.operator_position).into());
                }
                Ok(operand_1 / operand_2)
            }
            BinaryOperatorToken::Modulus => Ok(operand_1 % operand_2),
            BinaryOperatorToken::Exponent => {
                if operand_2.is_integer() {
                    return Ok(Pow::pow(operand_1, operand_2.numer()));
                }
                // TODO: Implement
                return Err(Positioned::new(Unimplemented, self.operator_position).into());
            }
        }
    }

    fn position(&self) -> Position {
        Position::from_span(
            self.operator_position.clone(),
            Position::from_span(self.operand_1.position(), self.operand_2.position()),
        )
    }
}

#[derive(Clone, Debug)]
struct FunctionNode {
    function_name: FunctionNameToken,
    function_name_position: Position,
    operands: Vec<SyntaxTreeNode>,
    operands_position: Position,
}

impl OperationNode for FunctionNode {
    fn execute(
        self: Box<Self>,
        mut maybe_vars: Option<&mut VariableStore>,
        mut maybe_db: Option<&mut SavedData>,
    ) -> Result<BigRational, CalculatorFailure> {
        let mut operands: Vec<BigRational> = Vec::new();
        for operand in self.operands {
            operands.push(operand.execute(maybe_vars.as_deref_mut(), maybe_db.as_deref_mut())?);
        }
        match self.function_name {
            FunctionNameToken::Max => {
                let mut operand_iter = operands.into_iter();
                let init = match operand_iter.next() {
                    Some(i) => i,
                    None => {
                        return Err(Positioned::new(
                            FunctionNeedsArguments(self.function_name),
                            self.function_name_position,
                        )
                        .into())
                    }
                };
                Ok(operand_iter.fold(init, max))
            }
            FunctionNameToken::Min => {
                let mut operand_iter = operands.into_iter();
                let init = match operand_iter.next() {
                    Some(i) => i,
                    None => {
                        return Err(Positioned::new(
                            FunctionNeedsArguments(self.function_name),
                            self.function_name_position,
                        )
                        .into())
                    }
                };
                Ok(operand_iter.fold(init, min))
            }
        }
    }

    fn position(&self) -> Position {
        Position::from_span(
            self.function_name_position.clone(),
            self.operands_position.clone(),
        )
    }
}

#[derive(Clone, Debug)]
struct ParenthesizedNode {
    open_position: Position,
    close_position: Position,
    node: SyntaxTreeNode,
}

impl OperationNode for ParenthesizedNode {
    fn execute(
        self: Box<Self>,
        maybe_vars: Option<&mut VariableStore>,
        maybe_db: Option<&mut SavedData>,
    ) -> Result<BigRational, CalculatorFailure> {
        self.node.execute(maybe_vars, maybe_db)
    }

    fn position(&self) -> Position {
        Position::from_span(self.open_position.clone(), self.close_position.clone())
    }
}

#[derive(Clone, Debug)]
enum SyntaxTreeNode {
    Number(Box<NumericNode>),
    Variable(Box<VariableNode>),
    Unary(Box<UnaryNode>),
    Binary(Box<BinaryNode>),
    Function(Box<FunctionNode>),
    Parenthesized(Box<ParenthesizedNode>),
}

impl SyntaxTreeNode {
    fn into_operation_node(self) -> Box<dyn OperationNode> {
        match self {
            SyntaxTreeNode::Number(n) => n,
            SyntaxTreeNode::Variable(n) => n,
            SyntaxTreeNode::Unary(n) => n,
            SyntaxTreeNode::Binary(n) => n,
            SyntaxTreeNode::Function(n) => n,
            SyntaxTreeNode::Parenthesized(n) => n,
        }
    }

    fn as_operation_node(&self) -> &dyn OperationNode {
        match self {
            SyntaxTreeNode::Number(n) => &**n,
            SyntaxTreeNode::Variable(n) => &**n,
            SyntaxTreeNode::Unary(n) => &**n,
            SyntaxTreeNode::Binary(n) => &**n,
            SyntaxTreeNode::Function(n) => &**n,
            SyntaxTreeNode::Parenthesized(n) => &**n,
        }
    }

    fn execute(
        self,
        maybe_vars: Option<&mut VariableStore>,
        maybe_db: Option<&mut SavedData>,
    ) -> Result<BigRational, CalculatorFailure> {
        self.into_operation_node().execute(maybe_vars, maybe_db)
    }

    fn position(&self) -> Position {
        self.as_operation_node().position()
    }
}

// Temporary structure that will help us construct the syntax tree.
// This will be used to make a vector of alternating operands and (binary) operators. This will
// allow us to iterate over the vector, applying the operators in the correct order in order to get
// the order of operations right.
#[derive(Clone, Debug)]
enum OperandOrOperator {
    Operand(SyntaxTreeNode),
    Operator(Positioned<BinaryOperatorToken>),
}

impl OperandOrOperator {
    fn is_operator(&self) -> bool {
        match self {
            OperandOrOperator::Operator(_) => true,
            _ => false,
        }
    }

    fn matches_operator(&self, other: BinaryOperatorToken) -> bool {
        match self {
            OperandOrOperator::Operator(Positioned { value, position: _ }) if *value == other => {
                true
            }
            _ => false,
        }
    }

    fn in_operator_slice(&self, slice: &[BinaryOperatorToken]) -> bool {
        match self {
            OperandOrOperator::Operator(Positioned { value, position: _ })
                if slice.contains(value) =>
            {
                true
            }
            _ => false,
        }
    }

    fn unwrap_operator(self) -> Positioned<BinaryOperatorToken> {
        match self {
            OperandOrOperator::Operator(o) => o,
            _ => panic!("Attempted to unwrap an operator from OperandOrOperator incorrectly"),
        }
    }

    fn is_operand(&self) -> bool {
        match self {
            OperandOrOperator::Operand(_) => true,
            _ => false,
        }
    }

    fn unwrap_operand(self) -> SyntaxTreeNode {
        match self {
            OperandOrOperator::Operand(o) => o,
            _ => panic!("Attempted to unwrap an operand from OperandOrOperator incorrectly"),
        }
    }
}

// Temporary structure that will help us construct the syntax tree.
// These are the possible values reasons that a valid expression ends.
#[derive(Clone, Debug)]
enum ExpressionEnd {
    Comma(Position),
    CloseParen(Position),
    InputEmpty,
}

// Temporary structure that will help us construct the syntax tree.
// We will read operators and operands out of the input token vector and return them via this enum.
#[derive(Clone, Debug)]
enum InputReadResult {
    Operand(SyntaxTreeNode),
    Operator(Positioned<BinaryOperatorToken>),
    End(ExpressionEnd),
}

impl From<ExpressionEnd> for InputReadResult {
    fn from(item: ExpressionEnd) -> Self {
        InputReadResult::End(item)
    }
}

// Temporary structure that will help us construct the syntax tree.
// When we expect to read an operand out of the input token vector, we'll read it into this enum.
#[derive(Clone, Debug)]
enum OperandReadResult {
    Operand(SyntaxTreeNode),
    End(ExpressionEnd),
}

/// This will describe a valid mathematical expression that optionally assigns its results to a
/// variable. Executing the syntax tree will consume it, assign to the specified variable (if
/// applicable), and return the result.
#[derive(Clone, Debug)]
pub struct SyntaxTree {
    maybe_result_var: Option<Positioned<String>>,
    root: SyntaxTreeNode,
}

impl SyntaxTree {
    pub fn new(
        mut input: VecDeque<Positioned<Token>>,
    ) -> Result<SyntaxTree, Positioned<SyntaxError>> {
        // Take the first two tokens. If they show that this is a variable assignment, use the
        // value from the token to set `maybe_result_var`. If this is not a variable assignment,
        // put the tokens back in the input.
        let first_token = input.pop_front();
        let second_token = input.pop_front();
        let maybe_result_var: Option<Positioned<String>> = match (first_token, second_token) {
            (
                Some(Positioned {
                    value: Token::Variable(var_name),
                    position,
                }),
                Some(Positioned {
                    value: Token::AssignmentOperator,
                    position: _,
                }),
            ) => Some(Positioned::new(var_name, position)),
            (first_token, second_token) => {
                if let Some(token) = second_token {
                    input.push_front(token);
                }
                if let Some(token) = first_token {
                    input.push_front(token);
                }
                None
            }
        };

        let root = match Self::read_expression(&mut input)? {
            (_, ExpressionEnd::Comma(p)) => {
                return Err(Positioned::new(UnexpectedToken(Token::Comma), p));
            }
            (_, ExpressionEnd::CloseParen(p)) => {
                return Err(Positioned::new(MismatchedCloseParen, p));
            }
            (None, ExpressionEnd::InputEmpty) => return Err(Positioned::new_raw(NoInput, 0, 0)),
            (Some(r), ExpressionEnd::InputEmpty) => r,
        };

        let st = SyntaxTree {
            maybe_result_var,
            root,
        };

        Ok(st)
    }

    fn read_expression(
        input: &mut VecDeque<Positioned<Token>>,
    ) -> Result<(Option<SyntaxTreeNode>, ExpressionEnd), Positioned<SyntaxError>> {
        // It's a little tricky to parse this out while also getting the order of operations right.
        // To make it easier, we are going to first break down the input into binary operators and
        // the syntax tree nodes that go between the binary operators. Then we can apply the order
        // operations to the list.
        let mut ooos: VecDeque<OperandOrOperator> = VecDeque::new();

        let expression_end: ExpressionEnd = loop {
            match Self::read_operand_or_operator(input)? {
                InputReadResult::Operand(o) => ooos.push_back(OperandOrOperator::Operand(o)),
                InputReadResult::Operator(o) => ooos.push_back(OperandOrOperator::Operator(o)),
                InputReadResult::End(e) => break e,
            }
        };

        // Before we can apply order of operations, we need fix the issue of tokenization having
        // assumed that all `-` characters are subtraction binary operators when some of them may be
        // unary negation operators.
        // To do this, we are going to take elements out of the "operand or operator" vector
        // (`ooos`) in reverse order and see if we can combine operands and operators in a way that
        // the resulting vector alternates between binary operators and operands.
        // Doing it in reverse order allows us to successfully combine multiple consecutive negation
        // operators.
        let mut temp: VecDeque<OperandOrOperator> = VecDeque::new();
        loop {
            let ooo = match ooos.pop_back() {
                Some(o) => o,
                None => break,
            };

            if ooo.is_operand()
                && !ooos.is_empty()
                && ooos[ooos.len() - 1].matches_operator(BinaryOperatorToken::Subtract)
                && (ooos.len() < 2 || ooos[ooos.len() - 2].is_operator())
            {
                let operator = ooos.pop_back().unwrap().unwrap_operator();
                let node = UnaryNode {
                    operator: UnaryOperatorToken::Negate,
                    operator_position: operator.position,
                    operand: ooo.unwrap_operand(),
                };
                // Put this back in `ooos`, not `temp`. This way we check again on the next loop if
                // there is another consecutive subtraction operator that ought to be converted into
                // a negation operator.
                ooos.push_back(OperandOrOperator::Operand(SyntaxTreeNode::Unary(Box::new(
                    node,
                ))));
            } else {
                temp.push_front(ooo);
            }
        }
        mem::swap(&mut temp, &mut ooos);

        // At this point, if the input was syntactically correct, `ooos` ought to be a vector of
        // alternating operands and operators starting and ending with an operand. We will go
        // through the list once for each type of operator, each time combining one type of operator
        // with the nodes surrounding it. Once the `for` loop exits, we should be down to a single
        // node.
        for ordered_operator in ORDERED_BINARY_OPERATORS {
            loop {
                let ooo = match ooos.pop_front() {
                    Some(o) => o,
                    None => break,
                };
                if ooo.is_operand()
                    && ooos.len() >= 2
                    && ooos[0].in_operator_slice(*ordered_operator)
                    && ooos[1].is_operand()
                {
                    let operand_1 = ooo.unwrap_operand();
                    let operator = ooos.pop_front().unwrap().unwrap_operator();
                    let operand_2 = ooos.pop_front().unwrap().unwrap_operand();

                    let node = BinaryNode {
                        operator: operator.value,
                        operator_position: operator.position,
                        operand_1,
                        operand_2,
                    };
                    // Put this back in `ooos`, not `temp`. This way we check again on the next loop
                    // if there is another consecutive operator that ought to be combined.
                    ooos.push_front(OperandOrOperator::Operand(SyntaxTreeNode::Binary(
                        Box::new(node),
                    )));
                } else {
                    temp.push_back(ooo);
                }
            }
            mem::swap(&mut temp, &mut ooos);
        }

        let root: Option<SyntaxTreeNode> =
            match (ooos.pop_front(), ooos.pop_front(), ooos.pop_front()) {
                (None, _, _) => None,
                (Some(OperandOrOperator::Operand(operand)), None, _) => Some(operand),
                (
                    Some(OperandOrOperator::Operand(_)),
                    Some(OperandOrOperator::Operator(operator)),
                    None,
                )
                | (
                    Some(OperandOrOperator::Operand(_)),
                    Some(OperandOrOperator::Operator(operator)),
                    Some(OperandOrOperator::Operator(_)),
                )
                | (Some(OperandOrOperator::Operator(operator)), _, _) => {
                    return Err(operator.map(|v| MissingOperand(v.into())));
                }
                (
                    Some(OperandOrOperator::Operand(operand_1)),
                    Some(OperandOrOperator::Operand(operand_2)),
                    _,
                ) => {
                    return Err(Positioned::new_between(
                        MissingOperator,
                        operand_1.position(),
                        operand_2.position(),
                    ));
                }
                (
                    Some(OperandOrOperator::Operand(_)),
                    Some(OperandOrOperator::Operator(operator)),
                    Some(OperandOrOperator::Operand(_)),
                ) => {
                    panic!("{} is not in ORDERED_BINARY_OPERATORS", operator.value);
                }
            };

        Ok((root, expression_end))
    }

    // Returns `None` if the input vector is empty or we are at the end of the expression.
    fn read_operand_or_operator(
        input: &mut VecDeque<Positioned<Token>>,
    ) -> Result<InputReadResult, Positioned<SyntaxError>> {
        let Positioned {
            value: token,
            position,
        } = match input.pop_front() {
            Some(i) => i,
            None => return Ok(ExpressionEnd::InputEmpty.into()),
        };

        let node: SyntaxTreeNode = match token {
            t @ Token::AssignmentOperator => {
                return Err(Positioned::new(UnexpectedToken(t), position));
            }
            Token::Comma => return Ok(ExpressionEnd::Comma(position).into()),
            Token::CloseParen => return Ok(ExpressionEnd::CloseParen(position).into()),
            Token::BinaryOperator(operator) => {
                return Ok(InputReadResult::Operator(Positioned::new(
                    operator, position,
                )));
            }
            Token::Variable(name) => {
                SyntaxTreeNode::Variable(Box::new(VariableNode { name, position }))
            }
            Token::Number(value) => {
                SyntaxTreeNode::Number(Box::new(NumericNode { value, position }))
            }
            Token::UnaryOperator(operator) => Self::read_unary_node(input, operator, position)?,
            Token::OpenParen => Self::read_parenthesized_node(input, position)?,
            Token::Function(name) => Self::read_function_node(input, name, position)?,
        };
        Ok(InputReadResult::Operand(node))
    }

    // Returns `None` if the input vector is empty or we are at the end of the expression.
    fn read_operand(
        input: &mut VecDeque<Positioned<Token>>,
    ) -> Result<OperandReadResult, Positioned<SyntaxError>> {
        match Self::read_operand_or_operator(input)? {
            InputReadResult::Operand(op) => Ok(OperandReadResult::Operand(op)),
            InputReadResult::Operator(op) => {
                if op.value == BinaryOperatorToken::Subtract {
                    let node =
                        Self::read_unary_node(input, UnaryOperatorToken::Negate, op.position)?;
                    Ok(OperandReadResult::Operand(node))
                } else {
                    Err(op.map(|v| UnexpectedToken(v.into())))
                }
            }
            InputReadResult::End(e) => Ok(OperandReadResult::End(e)),
        }
    }

    fn read_unary_node(
        input: &mut VecDeque<Positioned<Token>>,
        operator: UnaryOperatorToken,
        operator_position: Position,
    ) -> Result<SyntaxTreeNode, Positioned<SyntaxError>> {
        let operand = match Self::read_operand(input)? {
            OperandReadResult::Operand(operand) => operand,
            OperandReadResult::End(_) => {
                return Err(Positioned::new(
                    MissingOperand(operator.into()),
                    operator_position,
                ));
            }
        };
        Ok(SyntaxTreeNode::Unary(Box::new(UnaryNode {
            operator,
            operator_position,
            operand,
        })))
    }

    // Assumes that the open parenthesis token has already been pulled off the input vector.
    fn read_parenthesized_node(
        input: &mut VecDeque<Positioned<Token>>,
        open_position: Position,
    ) -> Result<SyntaxTreeNode, Positioned<SyntaxError>> {
        let (node, close_position) = match Self::read_expression(input)? {
            (Some(node), ExpressionEnd::CloseParen(close_position)) => (node, close_position),
            (None, ExpressionEnd::CloseParen(close_pos)) => {
                return Err(Positioned::new_span(EmptyParens, open_position, close_pos));
            }
            (_, ExpressionEnd::Comma(p)) => {
                return Err(Positioned::new(UnexpectedToken(Token::Comma), p));
            }
            (_, ExpressionEnd::InputEmpty) => {
                return Err(Positioned::new(MismatchedOpenParen, open_position));
            }
        };
        Ok(SyntaxTreeNode::Parenthesized(Box::new(ParenthesizedNode {
            open_position,
            close_position,
            node,
        })))
    }

    // Note that we do not validate function argument count when we build the syntax tree. We
    // validate it at execution time.
    fn read_function_node(
        input: &mut VecDeque<Positioned<Token>>,
        function_name: FunctionNameToken,
        function_name_position: Position,
    ) -> Result<SyntaxTreeNode, Positioned<SyntaxError>> {
        let post_fn_name_token = match input.pop_front() {
            None => {
                // We allow a function to be called with no parentheses. But, for consistency, we
                // always assume that we are reading a single argument in that case. Since we
                // neither found parentheses or an argument, this is an error.
                return Err(Positioned::new(
                    FunctionWithoutParensOrArgument(function_name),
                    function_name_position,
                ));
            }
            Some(t) => t,
        };
        // If the next token isn't an open parenthesis, get the next operand and return.
        match post_fn_name_token.value {
            Token::OpenParen => {}
            not_paren => {
                input.push_front(Positioned::new(not_paren, post_fn_name_token.position));
                let operand = match Self::read_operand(input)? {
                    OperandReadResult::Operand(o) => o,
                    OperandReadResult::End(_) => {
                        return Err(Positioned::new(
                            FunctionWithoutParensOrArgument(function_name),
                            function_name_position,
                        ));
                    }
                };
                let node = FunctionNode {
                    function_name,
                    function_name_position,
                    operands_position: operand.position(),
                    operands: vec![operand],
                };
                return Ok(SyntaxTreeNode::Function(Box::new(node)));
            }
        }

        let mut operands: Vec<SyntaxTreeNode> = Vec::new();
        // Read arguments until we find the close parenthesis.
        let mut maybe_comma_pos: Option<Position> = None;
        let close_paren_pos = loop {
            match Self::read_expression(input)? {
                (Some(operand), end) => {
                    operands.push(operand);
                    match end {
                        ExpressionEnd::Comma(pos) => maybe_comma_pos = Some(pos),
                        ExpressionEnd::CloseParen(pos) => break pos,
                        ExpressionEnd::InputEmpty => {
                            return Err(Positioned::new(
                                MismatchedOpenParen,
                                post_fn_name_token.position,
                            ));
                        }
                    }
                }
                (None, end) => match maybe_comma_pos {
                    Some(comma_pos) => {
                        return Err(Positioned::new(CommaWithoutOperandAfter, comma_pos));
                    }
                    None => match end {
                        ExpressionEnd::Comma(pos) => {
                            return Err(Positioned::new(CommaWithoutOperandBefore, pos));
                        }
                        ExpressionEnd::CloseParen(pos) => break pos,
                        ExpressionEnd::InputEmpty => {
                            return Err(Positioned::new(
                                MismatchedOpenParen,
                                post_fn_name_token.position,
                            ));
                        }
                    },
                },
            }
        };
        let node = FunctionNode {
            function_name,
            function_name_position,
            operands,
            operands_position: Position::from_span(post_fn_name_token.position, close_paren_pos),
        };
        Ok(SyntaxTreeNode::Function(Box::new(node)))
    }

    pub fn execute(
        self,
        maybe_input_history_id: Option<i64>,
        mut maybe_vars: Option<&mut VariableStore>,
        mut maybe_db: Option<&mut SavedData>,
    ) -> Result<BigRational, CalculatorFailure> {
        let result = self
            .root
            .execute(maybe_vars.as_deref_mut(), maybe_db.as_deref_mut())?;
        if let Some(result_var) = self.maybe_result_var {
            let var = Variable {
                name: result_var.value,
                value: result.clone(),
            };
            match maybe_vars {
                Some(vars) => vars.update(var, maybe_input_history_id, maybe_db)?,
                None => return Err(Positioned::new(NoVariableStore, result_var.position).into()),
            }
        }
        Ok(result)
    }
}

#[cfg(test)]
mod syntax_tree_tests {
    use crate::{
        error::SyntaxError,
        position::Positioned,
        syntax_tree::{SyntaxTree, SyntaxTreeNode},
        token::{
            BinaryOperatorToken::{self, Add, Divide, Exponent, Modulus, Multiply, Subtract},
            FunctionNameToken::{self, Max},
            ParsedInput, Tokenizer,
            UnaryOperatorToken::{self, Negate},
        },
    };
    use num::BigInt;
    use std::collections::VecDeque;

    fn str_to_syntax_tree(input: &str) -> Result<SyntaxTree, Positioned<SyntaxError>> {
        let tokenizer = Tokenizer::new();
        let tokens = match tokenizer.tokenize(input, 10).unwrap() {
            ParsedInput::Tokens(t) => t,
            ParsedInput::Command(_) => panic!(),
        };
        SyntaxTree::new(tokens.into())
    }

    fn assert_int(stn: SyntaxTreeNode, value: i64, start: usize, width: usize) {
        assert_eq!(stn.position().start, start);
        assert_eq!(stn.position().width, width);
        let node = match stn {
            SyntaxTreeNode::Number(n) => n,
            _ => panic!(),
        };
        assert_eq!(node.value.numer(), &BigInt::from(value));
        assert_eq!(node.value.denom(), &BigInt::from(1));
        assert_eq!(node.position.start, start);
        assert_eq!(node.position.width, width);
    }

    fn assert_var(stn: SyntaxTreeNode, value: &str, start: usize, width: usize) {
        assert_eq!(stn.position().start, start);
        assert_eq!(stn.position().width, width);
        let node = match stn {
            SyntaxTreeNode::Variable(n) => n,
            _ => panic!(),
        };
        assert_eq!(&node.name, value);
        assert_eq!(node.position.start, start);
        assert_eq!(node.position.width, width);
    }

    fn assert_binary_operator(
        stn: SyntaxTreeNode,
        operator: BinaryOperatorToken,
        operator_start: usize,
        operator_width: usize,
        expression_start: usize,
        expression_width: usize,
    ) -> (SyntaxTreeNode, SyntaxTreeNode) {
        assert_eq!(stn.position().start, expression_start);
        assert_eq!(stn.position().width, expression_width);
        let node = match stn {
            SyntaxTreeNode::Binary(n) => n,
            _ => panic!(),
        };
        assert_eq!(node.operator, operator);
        assert_eq!(node.operator_position.start, operator_start);
        assert_eq!(node.operator_position.width, operator_width);
        (node.operand_1, node.operand_2)
    }

    fn assert_parens(
        stn: SyntaxTreeNode,
        open_position: usize,
        close_position: usize,
    ) -> SyntaxTreeNode {
        assert_eq!(stn.position().start, open_position);
        assert_eq!(stn.position().width, close_position - open_position + 1);
        let node = match stn {
            SyntaxTreeNode::Parenthesized(n) => n,
            _ => panic!(),
        };
        assert_eq!(node.open_position.start, open_position);
        assert_eq!(node.open_position.width, 1);
        assert_eq!(node.close_position.start, close_position);
        assert_eq!(node.close_position.width, 1);
        node.node
    }

    fn assert_unary_operator(
        stn: SyntaxTreeNode,
        operator: UnaryOperatorToken,
        operator_start: usize,
        operator_width: usize,
        expression_start: usize,
        expression_width: usize,
    ) -> SyntaxTreeNode {
        assert_eq!(stn.position().start, expression_start);
        assert_eq!(stn.position().width, expression_width);
        let node = match stn {
            SyntaxTreeNode::Unary(n) => n,
            _ => panic!(),
        };
        assert_eq!(node.operator, operator);
        assert_eq!(node.operator_position.start, operator_start);
        assert_eq!(node.operator_position.width, operator_width);
        node.operand
    }

    fn assert_function(
        stn: SyntaxTreeNode,
        function_name: FunctionNameToken,
        name_start: usize,
        name_width: usize,
        operands_start: usize,
        operands_width: usize,
    ) -> VecDeque<SyntaxTreeNode> {
        let node = match stn {
            SyntaxTreeNode::Function(n) => n,
            _ => panic!(),
        };
        assert_eq!(node.function_name, function_name);
        assert_eq!(node.function_name_position.start, name_start);
        assert_eq!(node.function_name_position.width, name_width);
        assert_eq!(node.operands_position.start, operands_start);
        assert_eq!(node.operands_position.width, operands_width);
        node.operands.into()
    }

    #[test]
    fn lone_value() {
        let st = str_to_syntax_tree("123").unwrap();
        assert!(st.maybe_result_var.is_none());
        assert_int(st.root, 123, 0, 3);
    }

    #[test]
    fn lone_value_with_padding() {
        let st = str_to_syntax_tree("  123  ").unwrap();
        assert!(st.maybe_result_var.is_none());
        assert_int(st.root, 123, 2, 3);
    }

    #[test]
    fn lone_var() {
        let st = str_to_syntax_tree("$var").unwrap();
        assert!(st.maybe_result_var.is_none());
        assert_var(st.root, "$var", 0, 4);
    }

    #[test]
    fn consecutive_ints() {
        let error = str_to_syntax_tree("1 2").unwrap_err();
        match error.value {
            SyntaxError::MissingOperator => {}
            _ => panic!(),
        }
        assert_eq!(error.position.start, 1);
        assert_eq!(error.position.width, 1);
    }

    #[test]
    fn assignment() {
        let st = str_to_syntax_tree("$var=123").unwrap();
        match st.maybe_result_var {
            Some(var_name) => {
                assert_eq!(&var_name.value, "$var");
                assert_eq!(var_name.position.start, 0);
                assert_eq!(var_name.position.width, 4);
            }
            None => panic!(),
        }
        assert_int(st.root, 123, 5, 3);
    }

    #[test]
    fn addition() {
        let st = str_to_syntax_tree("1+2").unwrap();
        assert!(st.maybe_result_var.is_none());
        let (operand_1, operand_2) = assert_binary_operator(st.root, Add, 1, 1, 0, 3);
        assert_int(operand_1, 1, 0, 1);
        assert_int(operand_2, 2, 2, 1);
    }

    #[test]
    fn double_addition() {
        let st = str_to_syntax_tree("1+2+3").unwrap();
        assert!(st.maybe_result_var.is_none());
        let (operand_1_2, operand_3) = assert_binary_operator(st.root, Add, 3, 1, 0, 5);
        assert_int(operand_3, 3, 4, 1);
        let (operand_1, operand_2) = assert_binary_operator(operand_1_2, Add, 1, 1, 0, 3);
        assert_int(operand_1, 1, 0, 1);
        assert_int(operand_2, 2, 2, 1);
    }

    #[test]
    fn mixed_operator_chain() {
        let st = str_to_syntax_tree("1+2+3-4*5/6+7^8%9").unwrap();
        assert!(st.maybe_result_var.is_none());
        let (operand_1_6, operand_7_9) = assert_binary_operator(st.root, Add, 11, 1, 0, 17);
        let (operand_1_3, operand_4_6) = assert_binary_operator(operand_1_6, Subtract, 5, 1, 0, 11);
        let (operand_1_2, operand_3) = assert_binary_operator(operand_1_3, Add, 3, 1, 0, 5);
        assert_int(operand_3, 3, 4, 1);
        let (operand_1, operand_2) = assert_binary_operator(operand_1_2, Add, 1, 1, 0, 3);
        assert_int(operand_1, 1, 0, 1);
        assert_int(operand_2, 2, 2, 1);
        let (operand_4_5, operand_6) = assert_binary_operator(operand_4_6, Divide, 9, 1, 6, 5);
        assert_int(operand_6, 6, 10, 1);
        let (operand_4, operand_5) = assert_binary_operator(operand_4_5, Multiply, 7, 1, 6, 3);
        assert_int(operand_4, 4, 6, 1);
        assert_int(operand_5, 5, 8, 1);
        let (operand_7_8, operand_9) = assert_binary_operator(operand_7_9, Modulus, 15, 1, 12, 5);
        assert_int(operand_9, 9, 16, 1);
        let (operand_7, operand_8) = assert_binary_operator(operand_7_8, Exponent, 13, 1, 12, 3);
        assert_int(operand_7, 7, 12, 1);
        assert_int(operand_8, 8, 14, 1);
    }

    #[test]
    fn order_of_operations() {
        let st = str_to_syntax_tree("1*2+3*4^(5+6)").unwrap();
        assert!(st.maybe_result_var.is_none());
        let (operand_1_2, operand_3_6) = assert_binary_operator(st.root, Add, 3, 1, 0, 13);
        let (operand_1, operand_2) = assert_binary_operator(operand_1_2, Multiply, 1, 1, 0, 3);
        assert_int(operand_1, 1, 0, 1);
        assert_int(operand_2, 2, 2, 1);
        let (operand_3, operand_4_6) = assert_binary_operator(operand_3_6, Multiply, 5, 1, 4, 9);
        assert_int(operand_3, 3, 4, 1);
        let (operand_4, operand_paren) = assert_binary_operator(operand_4_6, Exponent, 7, 1, 6, 7);
        assert_int(operand_4, 4, 6, 1);
        let operand_5_6 = assert_parens(operand_paren, 8, 12);
        let (operand_5, operand_6) = assert_binary_operator(operand_5_6, Add, 10, 1, 9, 3);
        assert_int(operand_5, 5, 9, 1);
        assert_int(operand_6, 6, 11, 1);
    }

    #[test]
    fn negative_number() {
        let st = str_to_syntax_tree("-1").unwrap();
        assert!(st.maybe_result_var.is_none());
        let operand = assert_unary_operator(st.root, Negate, 0, 1, 0, 2);
        assert_int(operand, 1, 1, 1);
    }

    #[test]
    fn multiply_negated_number() {
        let st = str_to_syntax_tree("---1").unwrap();
        assert!(st.maybe_result_var.is_none());
        let operand = assert_unary_operator(st.root, Negate, 0, 1, 0, 4);
        let operand = assert_unary_operator(operand, Negate, 1, 1, 1, 3);
        let operand = assert_unary_operator(operand, Negate, 2, 1, 2, 2);
        assert_int(operand, 1, 3, 1);
    }

    #[test]
    fn subtraction() {
        let st = str_to_syntax_tree("1-2").unwrap();
        assert!(st.maybe_result_var.is_none());
        let (operand_1, operand_2) = assert_binary_operator(st.root, Subtract, 1, 1, 0, 3);
        assert_int(operand_1, 1, 0, 1);
        assert_int(operand_2, 2, 2, 1);
    }

    #[test]
    fn subtraction_of_multiply_negated_number() {
        let st = str_to_syntax_tree("1---2").unwrap();
        assert!(st.maybe_result_var.is_none());
        let (operand_1, operand_2) = assert_binary_operator(st.root, Subtract, 1, 1, 0, 5);
        assert_int(operand_1, 1, 0, 1);
        let operand_2 = assert_unary_operator(operand_2, Negate, 2, 1, 2, 3);
        let operand_2 = assert_unary_operator(operand_2, Negate, 3, 1, 3, 2);
        assert_int(operand_2, 2, 4, 1);
    }

    #[test]
    fn function_no_parens() {
        let st = str_to_syntax_tree("1+max 2").unwrap();
        assert!(st.maybe_result_var.is_none());
        let (operand_1, operand_max) = assert_binary_operator(st.root, Add, 1, 1, 0, 7);
        assert_int(operand_1, 1, 0, 1);
        let mut operands = assert_function(operand_max, Max, 2, 3, 6, 1);
        assert_eq!(operands.len(), 1);
        assert_int(operands.pop_front().unwrap(), 2, 6, 1);
    }

    #[test]
    fn function_empty_parens() {
        let st = str_to_syntax_tree("max()").unwrap();
        assert!(st.maybe_result_var.is_none());
        let operands = assert_function(st.root, Max, 0, 3, 3, 2);
        assert_eq!(operands.len(), 0);
    }

    #[test]
    fn function_expression_args() {
        let st = str_to_syntax_tree("max(1, -2, 3+4, max(5))").unwrap();
        assert!(st.maybe_result_var.is_none());
        let mut operands = assert_function(st.root, Max, 0, 3, 3, 20);
        assert_eq!(operands.len(), 4);
        assert_int(operands.pop_front().unwrap(), 1, 4, 1);
        let operand_2 = assert_unary_operator(operands.pop_front().unwrap(), Negate, 7, 1, 7, 2);
        assert_int(operand_2, 2, 8, 1);
        let (operand_3, operand_4) =
            assert_binary_operator(operands.pop_front().unwrap(), Add, 12, 1, 11, 3);
        assert_int(operand_3, 3, 11, 1);
        assert_int(operand_4, 4, 13, 1);
        let mut operands_max_2 = assert_function(operands.pop_front().unwrap(), Max, 16, 3, 19, 3);
        assert_eq!(operands_max_2.len(), 1);
        assert_int(operands_max_2.pop_front().unwrap(), 5, 20, 1);
    }
}
