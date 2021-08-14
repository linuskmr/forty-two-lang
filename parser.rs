//! The parser parses the tokens created by the lexer and and builds an abstract syntax tree
//! from them.
//!
use crate::ast;
use crate::ast::{AstNode, BinaryOperator};
use crate::error::{FTLError, FTLErrorKind};
use crate::position_container::{PositionRange, PositionRangeContainer, PositionContainer};
use crate::token::{Token, TokenType};
use std::iter::Peekable;
use std::convert::TryFrom;

/// A parser of tokens generated by its [Lexer].
pub struct Parser<TokenIter: Iterator<Item=Token>> {
    /// The source to read the [Token]s from.
    tokens: Peekable<TokenIter>,
}

/// The result of a parsing method.
type ParseResult = Result<AstNode, FTLError>;

impl<TokenIter: Iterator<Item=Token>> Parser<TokenIter> {
    /// Creates a new Parser from the given token iterator.
    pub fn new(tokens: TokenIter) -> Self {
        Self { tokens: tokens.peekable() }
    }

    fn current_position(&mut self) -> PositionRange {
        self.tokens.peek().map(|token| token.position).unwrap_or(PositionRange {
            line: 1,
            column: 1..=1
        })
    }

    /// Parses a binary expression, potentially followed by a sequence of (binary operator, primary expression).
    ///
    /// Note: Parentheses are a primary expression, so we don't have to worry about them here.
    fn parse_binary_expression(&mut self) -> ParseResult {
        let lhs = self.parse_primary_expression()?;
        let rhs = self.parse_binary_operation_rhs(None);
        todo!()
    }

    /// Parses a sequence of `(binary operation, primary expression)`. If this sequence is empty, it returns `lhs`.
    /// This function does not consume any tokens, if the binary operator has less precedence than `min_operator`.
    ///
    /// # Examples
    ///
    /// Think of the following expression: `a + b * c`. Then `lhs` contains `a`. This function reads the
    /// operator `+` and gets its precedence. Now the function parses the following primary expression as rhs, so
    /// here `b`. Then current_token contains `*`. This has a higher precedence than `+`, so the function recursively
    /// calls itself and parses everything on the right side until an operator is found, which precedence is not
    /// higher than `+`.
    fn parse_binary_operation_rhs(
        &mut self, min_operator: Option<BinaryOperator>
    ) -> Result<Option<(ast::BinaryOperator, AstNode)>, FTLError> {
        loop {
            let operator = match self.get_operator(&min_operator)? {
                Some(operator) => operator,
                // No operator, so no binary operation rhs
                None => return Ok(None)
            };

            // Parse the primary expression after the binary operator as rhs
            let mut rhs = self.parse_primary_expression()?;

            // Inspect next binary operator
            match self.tokens.peek() {
                Some(next_token) => {
                    let next_binary_operator = match ast::BinaryOperator::try_from(next_token) {
                        Ok(bin_op) => bin_op,
                        Err(_) =>
                    };
                    if operator_has_too_less_precedence(&operator, &Some(next_binary_operator)) {
                        // The next binary operator binds stronger with rhs than with current, so let
                        // it go with rhs.
                        rhs = self.parse_binary_operation_rhs(Some(operator.clone()))?;
                    }
                }
                None => (),
            };

            // Merge lhs and rhs and continue parsing
            lhs = Box::new(AstNode::BinaryExpression(ast::BinaryExpression {lhs, operator: operator.clone(), rhs }));
        }
    }

    fn get_operator(&mut self, min_operator: &Option<BinaryOperator>) -> Result<Option<BinaryOperator>, FTLError> {
        // Read the operator
        let operator = match self.tokens.peek() {
            // Expression ended here
            Some(Token { data: TokenType::Semicolon, .. }) | None => return Ok(None),
            // Try convert the token to a BinaryOperator
            Some(token) => ast::BinaryOperator::try_from(token)?,
        };
        if operator_has_too_less_precedence(&operator, &min_operator) {
            return Ok(None);
        }
        // Consume binary operator
        self.tokens.next();
        Ok(Some(operator))
    }

    fn parse_function_prototype(&mut self) -> ParseResult {
        // Get function name
        let function_name = match self.tokens.peek() {
            Some(Token { data: TokenType::Identifier(identifier), position: pos }) => {
                PositionRangeContainer { data: identifier.clone(), position: pos.clone() }
            }
            other => return Err(FTLError {
                kind: FTLErrorKind::IllegalToken,
                msg: format!("Expected identifier for function prototype, got {:?}", other),
                position: self.current_position(),
            }),
        };
        // Consume opening parentheses
        match self.tokens.next() {
            Some(Token { data: TokenType::OpeningParentheses, .. }) => (),
            other => return Err(FTLError {
                kind: FTLErrorKind::ExpectedSymbol,
                msg: format!("Expected `(` in function prototype, but was {:?}", other),
                position: self.current_position(),
            })
        }
        // Read list of arguments
        let mut arguments = Vec::new();
        while let Some(Token { data: TokenType::Identifier(arg_name), position }) = self.get_next_token().as_ref()? {
            arguments.push(PositionRangeContainer {
                data: arg_name.clone(),
                position: position.clone(),
            })
        }

        match &self.current_token.as_ref()? {
            Some(Token { data: TokenType::ClosingParentheses, .. }) => (),
            _ => return Err(FTLError {
                kind: FTLErrorKind::ExpectedSymbol,
                msg: format!("Expected `)` in function prototype, got {:?} instead", self.current_token),
                position: self.current_position(),
            })
        }

        self.get_next_token();
        Ok(ast::FunctionPrototype {
            name: function_name,
            args: arguments,
        })
    }

    fn parse_argument_list(&mut self) -> Result<Vec<ast::FunctionArgument>, FTLError> {
        let mut arguments = Vec::new();
        loop {
            let argument_name = match self.tokens.peek() {
                Some(Token {data: TokenType::Identifier(data), position}) => {
                    PositionRangeContainer {data: data.clone(), position: position.clone()}
                }
                other => return Err(FTLError {
                    kind: FTLErrorKind::IllegalToken,
                    msg: format!("Expected argument name, got {:?}", other),
                    position: self.current_position()
                })
            };
            self.next(); // Consume argument name
            match self.tokens.next() {
                Some(Token {data: TokenType::Colon, ..}) => (),
                other => return Err(FTLError {
                    kind: FTLErrorKind::IllegalToken,
                    msg: format!("Expected `:`, got {:?}", other),
                    position: self.current_position()
                })
            };
            let argument_type = match self.tokens.peek() {
                Some(Token {data: TokenType::Identifier(data), position}) => {
                    PositionRangeContainer {data: data.clone(), position: position.clone()}
                }
                other => return Err(FTLError {
                    kind: FTLErrorKind::IllegalToken,
                    msg: format!("Expected argument type, got {:?}", other),
                    position: self.current_position()
                })
            };
            self.tokens.next(); // Consume argument type
            arguments.push(ast::FunctionArgument {
                name: argument_name,
                typ: argument_type
            })
        }
        Ok(arguments)
    }

    fn parse_function_definition(&mut self) -> ParseResult {
        self.get_next_token();
        let func_proto = self.parse_function_prototype()?;
        let expr = self.parse_binary_expression()?;
        return Ok(Box::new(AstNode::Function(ast::Function {
            prototype: func_proto,
            body: expr,
        })));
    }

    /// Parses a number.
    fn parse_number(&mut self, number: PositionRangeContainer<f64>) -> ParseResult {
        let number = Ok(Box::new(AstNode::Number(number)));
        self.get_next_token(); // Eat number
        number
    }

    /// Parses a parentheses expression, like `(4 + 5)`.
    fn parse_parentheses(&mut self) -> ParseResult {
        self.get_next_token(); // Eat (
        let inner_expression = self.parse_binary_expression()?;
        match self.current_token.as_ref()? {
            Some(Token { data: TokenType::ClosingParentheses, .. }) => (), // Ok,
            _ => return Err(FTLError {
                kind: FTLErrorKind::ExpectedSymbol,
                msg: format!("Expected `)`"),
                position: self.current_position(),
            }),
        }
        self.get_next_token(); // Eat )
        return Ok(inner_expression);
    }

    /// Parses a variable.
    fn parse_variable(&mut self, variable: PositionRangeContainer<String>) -> ParseResult {
        Ok(AstNode::Variable(variable))
    }

    /// Collects the arguments of a function call, like `add(arg1, arg2)`.
    fn collect_function_call_arguments(&mut self) -> Result<Vec<Box<AstNode>>, FTLError> {
        if let Some(Token { data: TokenType::ClosingParentheses, .. }) = self.current_token.as_ref()? {
            // No arguments were passed
            return Ok(Vec::new());
        }
        let mut args = Vec::new();
        loop {
            args.push(self.parse_binary_expression()?);
            match self.current_token.as_ref()? {
                Some(Token { data: TokenType::ClosingParentheses, .. }) => {
                    // End of argument list
                    break;
                }
                Some(Token { data: TokenType::Comma, .. }) => {
                    // Ok, the argument list keeps going
                }
                _ => {
                    // Illegal token
                    return Err(FTLError {
                        kind: FTLErrorKind::ExpectedSymbol,
                        msg: format!("Expected `)` or `,` in argument list"),
                        position: self.current_position(),
                    });
                }
            };
        };
        Ok(args)
    }

    fn parse_extern_function(&mut self) -> ParseResult {
        assert_eq!(self.tokens.peek().map(|token| token.data), Some(TokenType::Identifier(String::from("extern"))));
        self.tokens.next(); // Consume extern token
        Ok(AstNode::FunctionPrototype(self.parse_function_prototype()?))
    }

    fn parse_top_level_expression(&mut self) -> ParseResult {
        let expression = self.parse_binary_expression()?;
        let function_proto = ast::FunctionPrototype {
            name: PositionRangeContainer {
                data: format!("__anonymous_function_{}", self.current_position().line),
                position: self.current_position(),
            },
            args: vec![],
        };
        Ok(Box::new(AstNode::Function(ast::Function { prototype: function_proto, body: expression })))
    }

    /// Parses a function call expression, like `add(2, 3)`.
    fn parse_function_call(&mut self, name: PositionRangeContainer<String>) -> ParseResult {
        assert_eq!(self.tokens.peek().map(|token| token.data), Some(TokenType::OpeningParentheses));
        self.tokens.next(); // Consume (
        let args = self.collect_function_call_arguments()?;
        assert_eq!(self.tokens.peek().map(|token| token.data), Some(TokenType::ClosingParentheses));
        self.tokens.next(); // Consume )
        Ok(AstNode::FunctionCall(ast::FunctionCall { name, args }))
    }

    /// Parses an identifier. The output is either a [Ast::FunctionCall] or an [Ast::Variable].
    fn parse_identifier_expression(&mut self, identifier: PositionRangeContainer<String>) -> ParseResult {
        match self.tokens.peek() {
            Some(Token { data: TokenType::OpeningParentheses, .. }) => {
                self.parse_function_call(identifier)
            }
            _ => self.parse_variable(identifier),
        }
    }

    /// The most basic type of an expression. Primary expression are either of type identifier, number or parentheses.
    fn parse_primary_expression(&mut self) -> ParseResult {
        let current_token = match self.tokens.peek() {
            Some(token) => token,
            None => return Err(FTLError {
                kind: FTLErrorKind::ExpectedExpression,
                msg: format!("Tried parsing a primary expression, but no expression found"),
                position: self.current_position()
            })
        };
        match current_token {
            Token { data: TokenType::Identifier(identifier), position } => {
                Some(self.parse_identifier_expression(PositionRangeContainer { data: identifier, position }))
            }
            Token { data: TokenType::Number(number), position } => {
                Some(self.parse_number(PositionRangeContainer { data: number, position }))
            }
            Token { data: TokenType::OpeningParentheses, .. } => {
                Some(self.parse_parentheses())
            },
            Token{data: TokenType::Semicolon, ..} => {
                None
            }
            _ => Some(Err(FTLError {
                kind: FTLErrorKind::ExpectedExpression,
                msg: format!("Expected primary expression, got {:?} instead", self.current_token),
                position: self.current_position(),
            })),
        }
    }
}

impl<L: Iterator<Item=Token>> Iterator for Parser<L> {
    type Item = ParseResult;

    fn next(&mut self) -> Option<Self::Item> {
        let token = match &self.get_next_token() {
            Ok(None) => return None, // Lexer drained
            Ok(Some(tok)) => tok,
            Err(err) => return Some(Err(err.clone())),
        };
        match token {
            Token{data: TokenType::Def, .. } => Some(self.parse_function_definition()),
            Token{data: TokenType::Extern, .. } => Some(self.parse_extern_function()),
            Token{data: TokenType::Semicolon, .. } => {
                // No_op (No operation)
                self.get_next_token();
                self.next()
            },
            _ => Some(self.parse_top_level_expression()),
        }
    }
}

/// Checks if operator has lesser `precedence` than `min_operator`.
fn operator_has_too_less_precedence(operator: &ast::BinaryOperator, min_operator: &Option<ast::BinaryOperator>) -> bool {
    min_operator.map(|min_op| operator.precedence() < min_op.precedence()).unwrap_or(false)
}