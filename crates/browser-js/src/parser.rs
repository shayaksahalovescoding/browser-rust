use crate::ast::{BinOp, Expr, Stmt, UnaryOp};
use crate::lexer::Token;

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    pub fn parse_program(&mut self) -> Result<Vec<Stmt>, String> {
        let mut stmts = Vec::new();
        while !self.at_eof() {
            while self.eat(&Token::Semicolon) {}
            if self.at_eof() {
                break;
            }
            stmts.push(self.parse_stmt()?);
        }
        Ok(stmts)
    }

    fn peek(&self) -> &Token {
        self.tokens.get(self.pos).unwrap_or(&Token::Eof)
    }

    fn peek_n(&self, n: usize) -> &Token {
        self.tokens.get(self.pos + n).unwrap_or(&Token::Eof)
    }

    fn at_eof(&self) -> bool {
        matches!(self.peek(), Token::Eof)
    }

    fn advance(&mut self) -> Token {
        let tok = self.peek().clone();
        if self.pos < self.tokens.len() {
            self.pos += 1;
        }
        tok
    }

    fn eat(&mut self, target: &Token) -> bool {
        if self.peek() == target {
            self.advance();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, target: &Token) -> Result<(), String> {
        if self.eat(target) {
            Ok(())
        } else {
            Err(format!("expected {target:?}, got {:?}", self.peek()))
        }
    }

    fn expect_ident(&mut self) -> Result<String, String> {
        match self.advance() {
            Token::Ident(s) => Ok(s),
            other => Err(format!("expected identifier, got {other:?}")),
        }
    }

    fn parse_stmt(&mut self) -> Result<Stmt, String> {
        match self.peek() {
            Token::Let | Token::Const | Token::Var => {
                self.advance();
                let name = self.expect_ident()?;
                let init = if self.eat(&Token::Eq) {
                    Some(self.parse_expr()?)
                } else {
                    None
                };
                self.eat(&Token::Semicolon);
                Ok(Stmt::VarDecl { name, init })
            }
            Token::Function => {
                self.advance();
                let name = self.expect_ident()?;
                let params = self.parse_param_list()?;
                let body = self.parse_block_stmts()?;
                Ok(Stmt::FuncDecl { name, params, body })
            }
            Token::LBrace => Ok(Stmt::Block(self.parse_block_stmts()?)),
            Token::If => {
                self.advance();
                self.expect(&Token::LParen)?;
                let cond = self.parse_expr()?;
                self.expect(&Token::RParen)?;
                let then_branch = Box::new(self.parse_stmt()?);
                let else_branch = if self.eat(&Token::Else) {
                    Some(Box::new(self.parse_stmt()?))
                } else {
                    None
                };
                Ok(Stmt::If {
                    cond,
                    then_branch,
                    else_branch,
                })
            }
            Token::While => {
                self.advance();
                self.expect(&Token::LParen)?;
                let cond = self.parse_expr()?;
                self.expect(&Token::RParen)?;
                let body = Box::new(self.parse_stmt()?);
                Ok(Stmt::While { cond, body })
            }
            Token::For => {
                self.advance();
                self.expect(&Token::LParen)?;
                let init = if self.eat(&Token::Semicolon) {
                    None
                } else {
                    let s = self.parse_stmt()?;
                    Some(Box::new(s))
                };
                let cond = if self.eat(&Token::Semicolon) {
                    None
                } else {
                    let e = self.parse_expr()?;
                    self.expect(&Token::Semicolon)?;
                    Some(e)
                };
                let step = if self.eat(&Token::RParen) {
                    None
                } else {
                    let e = self.parse_expr()?;
                    self.expect(&Token::RParen)?;
                    Some(e)
                };
                let body = Box::new(self.parse_stmt()?);
                Ok(Stmt::For {
                    init,
                    cond,
                    step,
                    body,
                })
            }
            Token::Return => {
                self.advance();
                let val = if matches!(self.peek(), Token::Semicolon | Token::RBrace | Token::Eof) {
                    None
                } else {
                    Some(self.parse_expr()?)
                };
                self.eat(&Token::Semicolon);
                Ok(Stmt::Return(val))
            }
            Token::Break => {
                self.advance();
                self.eat(&Token::Semicolon);
                Ok(Stmt::Break)
            }
            Token::Continue => {
                self.advance();
                self.eat(&Token::Semicolon);
                Ok(Stmt::Continue)
            }
            _ => {
                let expr = self.parse_expr()?;
                self.eat(&Token::Semicolon);
                Ok(Stmt::Expr(expr))
            }
        }
    }

    fn parse_block_stmts(&mut self) -> Result<Vec<Stmt>, String> {
        self.expect(&Token::LBrace)?;
        let mut stmts = Vec::new();
        while !matches!(self.peek(), Token::RBrace | Token::Eof) {
            while self.eat(&Token::Semicolon) {}
            if matches!(self.peek(), Token::RBrace | Token::Eof) {
                break;
            }
            stmts.push(self.parse_stmt()?);
        }
        self.expect(&Token::RBrace)?;
        Ok(stmts)
    }

    fn parse_param_list(&mut self) -> Result<Vec<String>, String> {
        self.expect(&Token::LParen)?;
        let mut params = Vec::new();
        if !self.eat(&Token::RParen) {
            loop {
                params.push(self.expect_ident()?);
                if !self.eat(&Token::Comma) {
                    break;
                }
            }
            self.expect(&Token::RParen)?;
        }
        Ok(params)
    }

    fn parse_expr(&mut self) -> Result<Expr, String> {
        self.parse_assign()
    }

    fn parse_assign(&mut self) -> Result<Expr, String> {
        // Check single-ident arrow fn: x => ...
        if let (Token::Ident(_), Token::Arrow) = (self.peek(), self.peek_n(1)) {
            let param = self.expect_ident()?;
            self.advance(); // =>
            let body = self.parse_arrow_body()?;
            return Ok(Expr::Function {
                params: vec![param],
                body,
            });
        }

        // Check parenthesized arrow fn: (a, b) => ...
        if matches!(self.peek(), Token::LParen) && self.looks_like_arrow_params() {
            let params = self.parse_param_list()?;
            self.expect(&Token::Arrow)?;
            let body = self.parse_arrow_body()?;
            return Ok(Expr::Function { params, body });
        }

        let left = self.parse_ternary()?;
        match self.peek() {
            Token::Eq => {
                self.advance();
                let value = self.parse_assign()?;
                Ok(Expr::Assign {
                    target: Box::new(left),
                    value: Box::new(value),
                })
            }
            Token::PlusEq => {
                self.advance();
                let rhs = self.parse_assign()?;
                let combined = Expr::Binary {
                    op: BinOp::Add,
                    left: Box::new(left.clone()),
                    right: Box::new(rhs),
                };
                Ok(Expr::Assign {
                    target: Box::new(left),
                    value: Box::new(combined),
                })
            }
            Token::MinusEq => {
                self.advance();
                let rhs = self.parse_assign()?;
                let combined = Expr::Binary {
                    op: BinOp::Sub,
                    left: Box::new(left.clone()),
                    right: Box::new(rhs),
                };
                Ok(Expr::Assign {
                    target: Box::new(left),
                    value: Box::new(combined),
                })
            }
            _ => Ok(left),
        }
    }

    fn looks_like_arrow_params(&self) -> bool {
        let mut idx = self.pos + 1;
        while let Some(tok) = self.tokens.get(idx) {
            match tok {
                Token::RParen => return matches!(self.tokens.get(idx + 1), Some(Token::Arrow)),
                Token::Ident(_) | Token::Comma => idx += 1,
                _ => return false,
            }
        }
        false
    }

    fn parse_arrow_body(&mut self) -> Result<Vec<Stmt>, String> {
        if matches!(self.peek(), Token::LBrace) {
            self.parse_block_stmts()
        } else {
            let expr = self.parse_assign()?;
            Ok(vec![Stmt::Return(Some(expr))])
        }
    }

    fn parse_ternary(&mut self) -> Result<Expr, String> {
        let cond = self.parse_or()?;
        if self.eat(&Token::Question) {
            let then_e = self.parse_expr()?;
            self.expect(&Token::Colon)?;
            let else_e = self.parse_assign()?;
            Ok(Expr::Ternary {
                cond: Box::new(cond),
                then_e: Box::new(then_e),
                else_e: Box::new(else_e),
            })
        } else {
            Ok(cond)
        }
    }

    fn parse_or(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_and()?;
        while self.eat(&Token::OrOr) {
            let right = self.parse_and()?;
            expr = Expr::Binary {
                op: BinOp::Or,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    fn parse_and(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_eq()?;
        while self.eat(&Token::AndAnd) {
            let right = self.parse_eq()?;
            expr = Expr::Binary {
                op: BinOp::And,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    fn parse_eq(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_cmp()?;
        loop {
            let op = match self.peek() {
                Token::EqEq => BinOp::EqEq,
                Token::EqEqEq => BinOp::EqEqEq,
                Token::BangEq => BinOp::NotEq,
                Token::BangEqEq => BinOp::NotEqEq,
                _ => break,
            };
            self.advance();
            let right = self.parse_cmp()?;
            expr = Expr::Binary {
                op,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    fn parse_cmp(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_add()?;
        loop {
            let op = match self.peek() {
                Token::Lt => BinOp::Lt,
                Token::Le => BinOp::Le,
                Token::Gt => BinOp::Gt,
                Token::Ge => BinOp::Ge,
                _ => break,
            };
            self.advance();
            let right = self.parse_add()?;
            expr = Expr::Binary {
                op,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    fn parse_add(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_mul()?;
        loop {
            let op = match self.peek() {
                Token::Plus => BinOp::Add,
                Token::Minus => BinOp::Sub,
                _ => break,
            };
            self.advance();
            let right = self.parse_mul()?;
            expr = Expr::Binary {
                op,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    fn parse_mul(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_unary()?;
        loop {
            let op = match self.peek() {
                Token::Star => BinOp::Mul,
                Token::Slash => BinOp::Div,
                Token::Percent => BinOp::Mod,
                _ => break,
            };
            self.advance();
            let right = self.parse_unary()?;
            expr = Expr::Binary {
                op,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    fn parse_unary(&mut self) -> Result<Expr, String> {
        match self.peek() {
            Token::Minus => {
                self.advance();
                Ok(Expr::Unary {
                    op: UnaryOp::Neg,
                    expr: Box::new(self.parse_unary()?),
                })
            }
            Token::Bang => {
                self.advance();
                Ok(Expr::Unary {
                    op: UnaryOp::Not,
                    expr: Box::new(self.parse_unary()?),
                })
            }
            Token::Typeof => {
                self.advance();
                Ok(Expr::Unary {
                    op: UnaryOp::Typeof,
                    expr: Box::new(self.parse_unary()?),
                })
            }
            _ => self.parse_postfix(),
        }
    }

    fn parse_postfix(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_primary()?;
        loop {
            match self.peek() {
                Token::Dot => {
                    self.advance();
                    let prop = self.expect_ident()?;
                    expr = Expr::Member {
                        object: Box::new(expr),
                        property: prop,
                    };
                }
                Token::LBracket => {
                    self.advance();
                    let idx = self.parse_expr()?;
                    self.expect(&Token::RBracket)?;
                    expr = Expr::Index {
                        object: Box::new(expr),
                        index: Box::new(idx),
                    };
                }
                Token::LParen => {
                    self.advance();
                    let mut args = Vec::new();
                    if !self.eat(&Token::RParen) {
                        loop {
                            args.push(self.parse_expr()?);
                            if !self.eat(&Token::Comma) {
                                break;
                            }
                        }
                        self.expect(&Token::RParen)?;
                    }
                    expr = Expr::Call {
                        callee: Box::new(expr),
                        args,
                    };
                }
                Token::PlusPlus => {
                    self.advance();
                    let inc = Expr::Binary {
                        op: BinOp::Add,
                        left: Box::new(expr.clone()),
                        right: Box::new(Expr::Number(1.0)),
                    };
                    expr = Expr::Assign {
                        target: Box::new(expr),
                        value: Box::new(inc),
                    };
                }
                Token::MinusMinus => {
                    self.advance();
                    let dec = Expr::Binary {
                        op: BinOp::Sub,
                        left: Box::new(expr.clone()),
                        right: Box::new(Expr::Number(1.0)),
                    };
                    expr = Expr::Assign {
                        target: Box::new(expr),
                        value: Box::new(dec),
                    };
                }
                _ => break,
            }
        }
        Ok(expr)
    }

    fn parse_primary(&mut self) -> Result<Expr, String> {
        match self.advance() {
            Token::Number(n) => Ok(Expr::Number(n)),
            Token::String(s) => Ok(Expr::String(s)),
            Token::True => Ok(Expr::Bool(true)),
            Token::False => Ok(Expr::Bool(false)),
            Token::Null => Ok(Expr::Null),
            Token::Undefined => Ok(Expr::Undefined),
            Token::Ident(s) => Ok(Expr::Ident(s)),
            Token::LParen => {
                let e = self.parse_expr()?;
                self.expect(&Token::RParen)?;
                Ok(e)
            }
            Token::Function => {
                if let Token::Ident(_) = self.peek() {
                    let _ = self.advance();
                }
                let params = self.parse_param_list()?;
                let body = self.parse_block_stmts()?;
                Ok(Expr::Function { params, body })
            }
            Token::LBracket => {
                let mut items = Vec::new();
                if !self.eat(&Token::RBracket) {
                    loop {
                        if matches!(self.peek(), Token::RBracket) {
                            break;
                        }
                        items.push(self.parse_expr()?);
                        if !self.eat(&Token::Comma) {
                            break;
                        }
                    }
                    self.expect(&Token::RBracket)?;
                }
                Ok(Expr::Array(items))
            }
            Token::LBrace => {
                let mut pairs = Vec::new();
                if !self.eat(&Token::RBrace) {
                    loop {
                        if matches!(self.peek(), Token::RBrace) {
                            break;
                        }
                        let key = match self.advance() {
                            Token::Ident(s) | Token::String(s) => s,
                            other => return Err(format!("expected object key, got {other:?}")),
                        };
                        self.expect(&Token::Colon)?;
                        let val = self.parse_expr()?;
                        pairs.push((key, val));
                        if !self.eat(&Token::Comma) {
                            break;
                        }
                    }
                    self.expect(&Token::RBrace)?;
                }
                Ok(Expr::Object(pairs))
            }
            other => Err(format!("unexpected token in expression: {other:?}")),
        }
    }
}
