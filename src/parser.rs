//! The parser parses the tokens created by the lexer and and builds an abstract syntax tree
//! from them.

use crate::ast;
use crate::ast::{AstNode, BinaryOperator, BasicDataType, Expression, BinaryExpression, DataType, FunctionPrototype, FunctionArgument, Function};
use crate::error::{FTLError, FTLErrorKind, ParseResult};
use crate::position_container::{PositionRange, PositionRangeContainer, PositionContainer};
use crate::token::{Token, TokenKind};
use std::iter::Peekable;
use std::convert::TryFrom;
use std::cmp::min;
use std::ops::Not;

/// A parser of tokens generated by its [Lexer].
pub struct Parser<TokenIter: Iterator<Item=Token>> {
    /// The source to read the [Token]s from.
    tokens: Peekable<TokenIter>,
}

impl<TokenIter: Iterator<Item=Token>> Parser<TokenIter> {
    /// Creates a new Parser from the given token iterator.
    pub fn new(tokens: TokenIter) -> Self {
        Self { tokens: tokens.peekable() }
    }

    /// Returns the position of the current token or [PositionRange::default()] if self.tokens.peek() returns None.
    fn current_position(&mut self) -> PositionRange {
        self.tokens.peek()
            .map(|token| token.position)
            .unwrap_or_default()
    }

    /// Parses a binary expression, potentially followed by a sequence of (binary operator, primary expression).
    ///
    /// Note: Parentheses are a primary expression, so we don't have to worry about them here.
    fn parse_binary_expression(&mut self) -> ParseResult<Expression> {
        let lhs: Expression = self.parse_primary_expression()?;
        self.parse_binary_operation_rhs(None, lhs)
    }

    /// Parses a sequence of `(binary operator, primary expression)`. If this sequence is empty, it returns `lhs`. If
    /// the binary operator has less precedence than `min_operator`.
    ///
    /// # Examples
    ///
    /// Think of the following expression: `a + b * c`. Then `lhs` contains `a`. This function reads the
    /// operator `+` and parses the following expression as `rhs`, so `b` here. Than `next_operator` is read and
    /// contains `*`. Because [BinaryOperator::Multiplication] (`*`) has a higher precedence than
    /// [BinaryOperator::Addition] (`+`). This causes this function recursively
    /// calls itself and parses everything on the right side until an operator is found, which precedence is not
    /// higher than `+`.
    fn parse_binary_operation_rhs(
        &mut self, min_operator: Option<&BinaryOperator>, lhs: Expression
    ) -> ParseResult<Expression> {
        // Make lhs mutable without enforcing the function caller that its lhs must be mutable
        let mut lhs = lhs;
        loop {
            // Read the operator after lhs and before rhs. On Err(...), return the error
            let operator = match self.parse_operator(min_operator, true)? {
                // Found an operator
                Some(operator) => operator,
                // Expression ended here
                None => return Ok(lhs),
            };
            // Parse the primary expression after operator as rhs
            let mut rhs: Expression = self.parse_primary_expression()?;
            // Inspect next operator
            self.parse_operator(min_operator, false).map(|next_operator|
                // If `next_operator` binds stronger with `rhs` than the current `operator`, let `rhs` go with
                // `next_operator`
                if next_operator > operator {
                    rhs = self.parse_binary_operation_rhs(Some(&operator.data), rhs)?;
                }
            )?;
            // Merge lhs and rhs and continue parsing
            lhs = Expression::BinaryExpression(BinaryExpression{
                lhs: Box::new(lhs),
                operator,
                rhs: Box::new(rhs)
            });
        }
    }

    /// Parses the next [BinaryOperator] from [Lexer::tokens]. Returns the [BinaryOperator] if it has more precedence than
    /// `min_operator`, otherwise [None]. If [Lexer::tokens] doesn't yield a [BinaryOperator], an [Err] is returned.
    ///
    /// # Arguments
    ///
    /// * `min_operator` - The parsed operator has to be greater than this minimum threshold. If [None], accept all
    /// operators.
    /// * `consume` - True if you want that the operator gets consumed, i.e. [Lexer::tokens.next()] will not yield the
    /// operator, but the token after the operator. False if you want that the operator don't gets consumed, i.e.
    /// [Lexer::tokens.next()] will yield the operator.
    fn parse_operator(
        &mut self, min_operator: Option<&BinaryOperator>, consume: bool
    ) -> ParseResult<Option<PositionRangeContainer<BinaryOperator>>> {
        // Read the operator
        let operator = match self.tokens.peek() {
            // No operator
            Some(Token{data: TokenKind::EndOfLine, .. }) | None => return Ok(None),
            Some(token) => PositionRangeContainer::<BinaryOperator>::try_from(token)?,
        };
        // Consume operator
        if consume { self.tokens.next(); }
        Ok(match min_operator {
            // min_operator not set. Accept every operator
            None => Some(operator),
            // Do not take operator with less or equal precedence compared to min_operator
            Some(min_op) => if operator > min_op { Some(operator) } else { None },
        })
    }

    /// Parses a [FunctionPrototype], i.e. a function name followed by opening parentheses, a list of arguments and
    /// closing parentheses.
    ///
    /// # Examples
    ///
    /// A valid function prototype is:
    /// ```text
    /// foo(x: int, y: float)
    /// ```
    fn parse_function_prototype(&mut self) -> ParseResult<FunctionPrototype> {
        // Get and consume function name
        let name = match self.tokens.next() {
            Some(Token { data: TokenKind::Identifier(identifier), .. }) => {
                PositionRangeContainer { data: identifier, position: pos }
            }
            other => return Err(FTLError {
                kind: FTLErrorKind::IllegalToken,
                msg: format!("Expected identifier for function prototype, got {:?}", other),
                position: self.current_position(),
            }),
        };
        // Check and consume opening parentheses
        match self.tokens.next() {
            Some(Token { data: TokenKind::OpeningParentheses, .. }) => (),
            other => return Err(FTLError {
                kind: FTLErrorKind::IllegalSymbol,
                msg: format!("Expected `(` in function prototype, got {:?}", other),
                position: self.current_position(),
            })
        }
        // Read list of arguments
        let mut args = self.parse_argument_list()?;
        // Check and consume closing parentheses
        match self.tokens.next() {
            Some(Token { data: TokenKind::ClosingParentheses, .. }) => (),
            other => return Err(FTLError {
                kind: FTLErrorKind::IllegalSymbol,
                msg: format!("Expected `)` in function prototype, got {:?}", other),
                position: self.current_position(),
            })
        }
        // TODO: Add parsing for the return value
        Ok(FunctionPrototype { name, args })
    }

    /// Pareses a list of arguments seperated by comma. This function parses arguments as long as they start with an
    /// identifier, so when reading a [TokenType::ClosingParentheses], this function returns.
    ///
    /// # Examples
    ///
    /// A valid argument list is:
    /// ```text
    /// x: int, y: float
    /// ```
    fn parse_argument_list(&mut self) -> ParseResult<Vec<FunctionArgument>> {
        let mut arguments = Vec::new();
        // Check if argument list starts with identifier. If not, the argument list is finished
        if let Some(Token {data: TokenKind::Identifier(_), ..}) = self.tokens.peek() {} else { return Ok(arguments) }
        // Collect all arguments
        loop {
            // Get and consume argument name
            let name = match self.tokens.next() {
                Some(Token {data: TokenKind::Identifier(data), position}) => {
                    PositionRangeContainer {data, position}
                }
                other => return Err(FTLError {
                    kind: FTLErrorKind::IllegalToken,
                    msg: format!("Expected argument name, got {:?}", other),
                    position: self.current_position()
                })
            };
            // Check and consume colon
            match self.tokens.next() {
                Some(Token {data: TokenKind::Colon, ..}) => (),
                other => return Err(FTLError {
                    kind: FTLErrorKind::IllegalToken,
                    msg: format!("Expected `:` between argument name and type, got {:?}", other),
                    position: self.current_position()
                })
            };
            // Get and consume argument type
            let data_type = self.parse_type()?;
            arguments.push(FunctionArgument { name, data_type });
            // Check and consume comma
            match self.tokens.peek() {
                Some(Token {data: TokenKind::Comma, ..}) => self.tokens.next(),
                _ => break // No comma after this argument means this is the last argument
            };
        }
        Ok(arguments)
    }

    /// Parses a [DataType]. A [DataType] is either a
    /// * basic data type (like `int` or `float`),
    /// * pointer to a data type (like `ptr int`),
    /// * user defined data type / struct (like `Person`).
    fn parse_type(&mut self) -> ParseResult<PositionRangeContainer<DataType>> {
        match self.tokens.next() {
            Some(Token {data: TokenKind::Identifier(typeStr), position: ptr_position }) if typeStr == "ptr" => {
                // Pointer
                // Recursively call parse_type() to parse the type the pointer points to. This recursive calling
                // enables types like `ptr ptr int`.
                let type_to_point_to = self.parse_type()?;
                Ok(PositionRangeContainer {
                    data: type_to_point_to.data,
                    position: PositionRange {
                        line: ptr_position.line,
                        column: *ptr_position.column.start()..=*type_to_point_to.position.column.end()
                    }
                })
            }
            Some(Token {data: TokenKind::Identifier(typeStr), position}) => {
                if let Ok(basic_data_type) = BasicDataType::try_from(&typeStr) {
                    // Basic data type
                    Ok(PositionRangeContainer {
                        data: ast::DataType::Basic(basic_data_type),
                        position
                    })
                } else {
                    // User defined data type / struct
                    Ok(PositionRangeContainer {
                        data: DataType::Struct(typeStr),
                        position
                    })
                }
            }
            other => Err(FTLError {
                kind: FTLErrorKind::IllegalToken,
                msg: format!("Expected argument type, got {:?}", other),
                position: self.current_position()
            })
        }
    }

    /// Parses a [Function] definition, i.e. a [FunctionPrototype] followed by the function body (an [Expression]).
    fn parse_function_definition(&mut self) -> ParseResult<Function> {
        let prototype = self.parse_function_prototype()?;
        let body = self.parse_binary_expression()?;
        return Ok(Function { prototype, body });
    }

    /// Parses a [Number], i.e. simply converts a [TokenKind::Number] from [Lexer::tokens.next()] to an [Number].
    ///
    /// # Panics
    ///
    /// Panics if [Lexer::tokens.next()] yields a [Token] which has not [TokenKind::Number], so test this before
    /// calling this function with [Lexer::tokens.peek()]
    fn parse_number(&mut self) -> ParseResult<Number> {
        Ok(match self.tokens.next() {
            Some(Token {data: TokenKind::Number(number), position}) => {
                ast::Number { data: number, position }
            },
            _ => panic!("parse_number() expected number token"),
        })
    }

    /// Parses a parentheses expression, i.e. a [TokenKind::OpeningParentheses] followed by an inner [Expression] and
    /// a final [TokenKind::ClosingParentheses]. The parentheses are not present in the AST, but are expressed by the
    /// AST structure.
    ///
    /// # Examples
    ///
    /// Valid parentheses expression are:
    /// ```text
    /// (40 + 2)
    /// (42 - answer_to_everything + 42)
    /// ```
    ///
    /// Not valid parentheses expression are:
    /// ```text
    /// (40 +2
    /// 40 + 2)
    /// ```
    fn parse_parentheses(&mut self) -> ParseResult<Expression> {
        assert_eq!(self.tokens.next().map(|token| token.data), Some(TokenKind::OpeningParentheses));
        let inner_expression = self.parse_binary_expression()?;
        match self.tokens.next() {
            Some(Token { data: TokenKind::ClosingParentheses, .. }) => (),
            other => return Err(FTLError {
                kind: FTLErrorKind::IllegalSymbol,
                msg: format!("Expected `)`, got {:?}", other),
                position: self.current_position(),
            }),
        }
        return Ok(inner_expression);
    }

    /// Parses a variable, i.e. does checks on the provided `identifier` and if they were successful, returns it.
    fn parse_variable(&mut self, identifier: PositionRangeContainer<String>) -> ParseResult<PositionRangeContainer<String>> {
        assert!(!identifier.data.is_empty()); // identifier can't be empty, because who should produce an empty token?
        Ok(identifier)
    }

    /// Parses an extern function, i.e. an [TokenKind::Extern] followed by a [FunctionPrototype] without a body.
    ///
    /// # Examples
    ///
    /// A valid declaration of an extern function for the
    /// [write syscall in libc](https://man7.org/linux/man-pages/man2/write.2.html) is:
    /// ```text
    /// extern write(fd: int, buf: ptr char, count: uint64)
    /// ```
    fn parse_extern_function(&mut self) -> ParseResult<ast::FunctionPrototype> {
        assert_eq!(self.tokens.next().map(|token| token.data), Some(TokenKind::Extern));
        self.parse_function_prototype()
    }

    /// Parses a top level expression, so this is the entry point of an ftl program. In the moment an ftl program is
    /// only one binary expression, which gets wrapped in a main function. This will change in the future.
    fn parse_top_level_expression(&mut self) -> ParseResult<Function> {
        let expression = self.parse_binary_expression()?;
        let function_prototype = ast::FunctionPrototype {
            name: PositionRangeContainer {
                data: format!("__main_line_{}", self.current_position().line),
                position: self.current_position(),
            },
            args: Vec::new(),
        };
        Ok(ast::Function { prototype: function_prototype, body: expression })
    }

    /// Parses a function call expression, like `add(2, 3)`.
    fn parse_function_call(&mut self, name: PositionRangeContainer<String>) -> ParseResult<ast::FunctionCall> {
        // Check and consume opening parentheses
        assert_eq!(self.tokens.next().map(|token| token.data), Some(TokenKind::OpeningParentheses));
        let args = self.parse_argument_list()?;
        // Check and consume closing parentheses
        assert_eq!(self.tokens.next().map(|token| token.data), Some(TokenKind::ClosingParentheses));
        Ok(ast::FunctionCall { name, args })
    }

    /// Parses an identifier. The output is either a [ast::Expression::FunctionCall] or an [ast::Expression::Variable].
    fn parse_identifier_expression(&mut self) -> ParseResult<ast::Expression> {
        let identifier = match self.tokens.next() {
            Some(Token{ data: TokenKind::Identifier(identifier), position}) => {
                PositionRangeContainer { data: identifier, position: position.into() }
            },
            _ => panic!("parse_identifier_expression() called on non-identifier")
        };
        match self.tokens.peek() {
            Some(Token { data: TokenKind::OpeningParentheses, .. }) => {
                // Identifier is followed by an opening parentheses, so it must be a function call
                let function_call = self.parse_function_call(identifier)?;
                Ok(ast::Expression::FunctionCall(function_call))
            }
            _ => {
                // Identifier is followed by something else, so it is a variable
                let variable = self.parse_variable(identifier)?;
                Ok(ast::Expression::Variable(variable))
            },
        }
    }

    /// The most basic type of an expression. Primary expression are either of type identifier, number or parentheses.
    fn parse_primary_expression(&mut self) -> ParseResult<ast::Expression> {
        let token = self.tokens.peek().ok_or(FTLError {
            kind: FTLErrorKind::ExpectedExpression,
            msg: format!("Tried parsing a primary expression, but no expression found"),
            position: self.current_position()
        })?;
        match token {
            Token { data: TokenKind::Identifier(_), .. } => {
                let identifier_expression = self.parse_identifier_expression()?;
                Ok(identifier_expression)
            }
            Token { data: TokenKind::Number(_), .. } => {
                let number = self.parse_number()?;
                Ok(ast::Expression::Number(number))
            }
            Token { data: TokenKind::OpeningParentheses, .. } => {
                let binary_expression = self.parse_parentheses()?;
                Ok(ast::Expression::BinaryExpression(binary_expression))
            },
            _ => return Err(FTLError {
                kind: FTLErrorKind::ExpectedExpression,
                msg: format!("Expected primary expression, got {:?} instead", self.current_token),
                position: self.current_position(),
            }),
        };
        todo!()
    }
}

impl<L: Iterator<Item=Token>> Iterator for Parser<L> {
    type Item = ParseResult<ast::AstNode>;

    fn next(&mut self) -> Option<Self::Item> {
        let token = match &self.get_next_token() {
            Ok(None) => return None, // Lexer drained
            Ok(Some(tok)) => tok,
            Err(err) => return Some(Err(err.clone())),
        };
        match token {
            Token{data: TokenKind::Def, .. } => Some(self.parse_function_definition()),
            Token{data: TokenKind::Extern, .. } => Some(self.parse_extern_function()),
            Token{data: TokenKind::Semicolon, .. } => {
                // No_op (No operation)
                self.get_next_token();
                self.next()
            },
            _ => Some(self.parse_top_level_expression()),
        }
    }
}