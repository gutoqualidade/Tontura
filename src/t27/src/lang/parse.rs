//! TENERURA — sintaxe do subconjunto de Rust para T27.
//!
//! Gramática (EBNF informal):
//!
//! ```text
//! programa   := item*
//! item       := 'fn' ident '(' params? ')' ('->' 'word')? bloco
//! params     := param (',' param)*
//! param      := ident ':' 'word'
//! bloco      := '{' comando* '}'
//! comando    := 'let' 'mut'? ident (':' 'word')? '=' expr ';'
//!             | ident '=' expr ';'
//!             | 'if' expr bloco ('else' bloco)?
//!             | 'while' expr bloco
//!             | 'return' expr? ';'
//!             | 'match' expr '{' braço* '}'
//!             | expr ';'
//! braço      := padrao ('|' padrao)* '=>' bloco
//! padrao     := '-'? número | '_'
//! expr       := or
//! or         := and ('||' and)*
//! and        := cmp ('&&' cmp)*
//! cmp        := soma (('=='|'!='|'<'|'<='|'>'|'>=') soma)*
//! soma       := termo (('+'|'-') termo)*
//! termo      := unário (('*'|'/'|'%') unário)*
//! unário     := ('-'|'!') unário | primário
//! primário   := número | 'true' | 'false' | ident | chamada | '(' expr ')'
//! chamada    := ident '(' (expr (',' expr)*)? ')'
//! ```

use super::lex::{Tok, Token};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Not,
}

#[derive(Debug)]
pub enum Expr {
    Num(i64),
    Var(String, usize, usize),
    Call(String, Vec<Expr>, usize, usize),
    Unary(UnOp, Box<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
}

#[derive(Debug)]
pub enum Stmt {
    Let { name: String, mutable: bool, init: Expr, line: usize, col: usize },
    Assign { name: String, value: Expr, line: usize, col: usize },
    If { cond: Expr, then: Block, els: Option<Block> },
    While { cond: Expr, body: Block },
    Return { value: Option<Expr> },
    Match { scrutinee: Expr, arms: Vec<Arm>, line: usize },
    Expr(Expr),
}

#[derive(Debug)]
pub struct Block {
    pub stmts: Vec<Stmt>,
}

#[derive(Debug)]
pub enum Pat {
    Lit(i64),
    Wild,
}

#[derive(Debug)]
pub struct Arm {
    pub pats: Vec<Pat>,
    pub body: Block,
}

#[derive(Debug)]
pub struct Fn {
    pub name: String,
    pub params: Vec<String>,
    pub body: Block,
    pub line: usize,
    pub col: usize,
}

#[derive(Debug)]
pub struct Program {
    pub fns: Vec<Fn>,
}

pub struct ParseError {
    pub line: usize,
    pub col: usize,
    pub msg: String,
}

pub struct Parser {
    toks: Vec<Token>,
    i: usize,
}

type R<T> = Result<T, ParseError>;

impl Parser {
    pub fn new(toks: Vec<Token>) -> Self {
        Parser { toks, i: 0 }
    }

    fn peek(&self) -> &Tok {
        &self.toks[self.i].tok
    }

    fn peek_at(&self, k: usize) -> &Tok {
        let j = (self.i + k).min(self.toks.len() - 1);
        &self.toks[j].tok
    }

    fn pos(&self) -> (usize, usize) {
        let t = &self.toks[self.i];
        (t.line, t.col)
    }

    fn err<T>(&self, msg: impl Into<String>) -> R<T> {
        let (line, col) = self.pos();
        Err(ParseError { line, col, msg: msg.into() })
    }

    fn bump(&mut self) -> Tok {
        let t = self.toks[self.i].tok.clone();
        if self.i + 1 < self.toks.len() {
            self.i += 1;
        }
        t
    }

    fn eat(&mut self, t: &Tok) -> bool {
        if self.peek() == t {
            self.bump();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, t: &Tok, what: &str) -> R<()> {
        if self.eat(t) {
            Ok(())
        } else {
            self.err(format!("esperava {what}, achei {:?}", self.peek()))
        }
    }

    fn ident(&mut self) -> R<String> {
        match self.peek().clone() {
            Tok::Ident(s) => {
                self.bump();
                Ok(s)
            }
            other => self.err(format!("esperava um nome, achei {other:?}")),
        }
    }

    pub fn program(&mut self) -> R<Program> {
        let mut fns = Vec::new();
        while self.peek() != &Tok::Eof {
            fns.push(self.function()?);
        }
        Ok(Program { fns })
    }

    fn function(&mut self) -> R<Fn> {
        let (line, col) = self.pos();
        self.expect(&Tok::Fn, "'fn'")?;
        let name = self.ident()?;
        self.expect(&Tok::LParen, "'('")?;
        let mut params = Vec::new();
        if self.peek() != &Tok::RParen {
            loop {
                let p = self.ident()?;
                self.expect(&Tok::Colon, "':'")?;
                self.expect(&Tok::Word, "o tipo 'word'")?;
                params.push(p);
                if !self.eat(&Tok::Comma) {
                    break;
                }
            }
        }
        self.expect(&Tok::RParen, "')'")?;
        if self.eat(&Tok::Arrow) {
            self.expect(&Tok::Word, "o tipo 'word'")?;
        }
        let body = self.block()?;
        Ok(Fn { name, params, body, line, col })
    }

    fn block(&mut self) -> R<Block> {
        self.expect(&Tok::LBrace, "'{'")?;
        let mut stmts = Vec::new();
        while self.peek() != &Tok::RBrace {
            if self.peek() == &Tok::Eof {
                return self.err("bloco sem '}'");
            }
            stmts.push(self.stmt()?);
        }
        self.expect(&Tok::RBrace, "'}'")?;
        Ok(Block { stmts })
    }

    fn stmt(&mut self) -> R<Stmt> {
        match self.peek().clone() {
            Tok::Let => {
                let (line, col) = self.pos();
                self.bump();
                let mutable = self.eat(&Tok::Mut);
                let name = self.ident()?;
                if self.eat(&Tok::Colon) {
                    self.expect(&Tok::Word, "o tipo 'word'")?;
                }
                self.expect(&Tok::Assign, "'='")?;
                let init = self.expr()?;
                self.expect(&Tok::Semi, "';'")?;
                Ok(Stmt::Let { name, mutable, init, line, col })
            }
            Tok::If => {
                self.bump();
                let cond = self.expr()?;
                let then = self.block()?;
                let els = if self.eat(&Tok::Else) {
                    if self.peek() == &Tok::If {
                        // else if: açúcar para else { if ... }
                        let nested = self.stmt()?;
                        Some(Block { stmts: vec![nested] })
                    } else {
                        Some(self.block()?)
                    }
                } else {
                    None
                };
                Ok(Stmt::If { cond, then, els })
            }
            Tok::While => {
                self.bump();
                let cond = self.expr()?;
                let body = self.block()?;
                Ok(Stmt::While { cond, body })
            }
            Tok::Return => {
                self.bump();
                let value = if self.peek() == &Tok::Semi { None } else { Some(self.expr()?) };
                self.expect(&Tok::Semi, "';'")?;
                Ok(Stmt::Return { value })
            }
            Tok::Match => {
                let (line, _) = self.pos();
                self.bump();
                let scrutinee = self.expr()?;
                self.expect(&Tok::LBrace, "'{'")?;
                let mut arms = Vec::new();
                while self.peek() != &Tok::RBrace {
                    if self.peek() == &Tok::Eof {
                        return self.err("match sem '}'");
                    }
                    let mut pats = vec![self.pattern()?];
                    while self.eat(&Tok::Pipe) {
                        pats.push(self.pattern()?);
                    }
                    self.expect(&Tok::FatArrow, "'=>'")?;
                    let body = self.block()?;
                    let _ = self.eat(&Tok::Comma);
                    arms.push(Arm { pats, body });
                }
                self.expect(&Tok::RBrace, "'}'")?;
                Ok(Stmt::Match { scrutinee, arms, line })
            }
            Tok::Ident(name) if self.peek_at(1) == &Tok::Assign => {
                let (line, col) = self.pos();
                self.bump();
                self.bump();
                let value = self.expr()?;
                self.expect(&Tok::Semi, "';'")?;
                Ok(Stmt::Assign { name, value, line, col })
            }
            _ => {
                let e = self.expr()?;
                self.expect(&Tok::Semi, "';'")?;
                Ok(Stmt::Expr(e))
            }
        }
    }

    fn pattern(&mut self) -> R<Pat> {
        if self.eat(&Tok::Underscore) {
            return Ok(Pat::Wild);
        }
        let neg = self.eat(&Tok::Minus);
        match self.peek().clone() {
            Tok::Num(v) => {
                self.bump();
                Ok(Pat::Lit(if neg { -v } else { v }))
            }
            other => self.err(format!("esperava um padrão (número ou '_'), achei {other:?}")),
        }
    }

    pub fn expr(&mut self) -> R<Expr> {
        self.or()
    }

    fn or(&mut self) -> R<Expr> {
        let mut e = self.and()?;
        while self.eat(&Tok::OrOr) {
            let r = self.and()?;
            e = Expr::Binary(BinOp::Or, Box::new(e), Box::new(r));
        }
        Ok(e)
    }

    fn and(&mut self) -> R<Expr> {
        let mut e = self.cmp()?;
        while self.eat(&Tok::AndAnd) {
            let r = self.cmp()?;
            e = Expr::Binary(BinOp::And, Box::new(e), Box::new(r));
        }
        Ok(e)
    }

    fn cmp(&mut self) -> R<Expr> {
        let mut e = self.sum()?;
        loop {
            let op = match self.peek() {
                Tok::EqEq => BinOp::Eq,
                Tok::Ne => BinOp::Ne,
                Tok::Lt => BinOp::Lt,
                Tok::Le => BinOp::Le,
                Tok::Gt => BinOp::Gt,
                Tok::Ge => BinOp::Ge,
                _ => break,
            };
            self.bump();
            let r = self.sum()?;
            e = Expr::Binary(op, Box::new(e), Box::new(r));
        }
        Ok(e)
    }

    fn sum(&mut self) -> R<Expr> {
        let mut e = self.term()?;
        loop {
            let op = match self.peek() {
                Tok::Plus => BinOp::Add,
                Tok::Minus => BinOp::Sub,
                _ => break,
            };
            self.bump();
            let r = self.term()?;
            e = Expr::Binary(op, Box::new(e), Box::new(r));
        }
        Ok(e)
    }

    fn term(&mut self) -> R<Expr> {
        let mut e = self.unary()?;
        loop {
            let op = match self.peek() {
                Tok::Star => BinOp::Mul,
                Tok::Slash => BinOp::Div,
                Tok::Percent => BinOp::Rem,
                _ => break,
            };
            self.bump();
            let r = self.unary()?;
            e = Expr::Binary(op, Box::new(e), Box::new(r));
        }
        Ok(e)
    }

    fn unary(&mut self) -> R<Expr> {
        if self.eat(&Tok::Minus) {
            return Ok(Expr::Unary(UnOp::Neg, Box::new(self.unary()?)));
        }
        if self.eat(&Tok::Bang) {
            return Ok(Expr::Unary(UnOp::Not, Box::new(self.unary()?)));
        }
        self.primary()
    }

    fn primary(&mut self) -> R<Expr> {
        match self.peek().clone() {
            Tok::Num(v) => {
                self.bump();
                Ok(Expr::Num(v))
            }
            Tok::True => {
                self.bump();
                Ok(Expr::Num(1))
            }
            Tok::False => {
                self.bump();
                Ok(Expr::Num(0))
            }
            Tok::LParen => {
                self.bump();
                let e = self.expr()?;
                self.expect(&Tok::RParen, "')'")?;
                Ok(e)
            }
            Tok::Ident(name) => {
                let (line, col) = self.pos();
                self.bump();
                if self.eat(&Tok::LParen) {
                    let mut args = Vec::new();
                    if self.peek() != &Tok::RParen {
                        loop {
                            args.push(self.expr()?);
                            if !self.eat(&Tok::Comma) {
                                break;
                            }
                        }
                    }
                    self.expect(&Tok::RParen, "')'")?;
                    Ok(Expr::Call(name, args, line, col))
                } else {
                    Ok(Expr::Var(name, line, col))
                }
            }
            other => self.err(format!("esperava uma expressão, achei {other:?}")),
        }
    }
}
