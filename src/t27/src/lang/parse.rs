//! TENERURA — sintaxe do subconjunto de Rust para T27.
//!
//! Gramática (EBNF informal):
//!
//! ```text
//! programa   := item*
//! item       := 'fn' ident '(' params? ')' ('->' tipo)? bloco
//!             | 'static' 'mut' ident ':' ('word' | '[' 'word' ';' número ']') ';'
//! tipo       := 'word' | 'bool' | 'trit'
//! params     := param (',' param)*
//! param      := ident ':' tipo
//! bloco      := '{' comando* '}'
//! comando    := 'let' 'mut'? ident (':' tipo)? '=' expr ';'
//!             | ident '=' expr ';'
//!             | ident '[' expr ']' '=' expr ';'
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
//! cmp        := kou (('=='|'!='|'<'|'<='|'>'|'>=') kou)*
//! kou        := ke ('|' ke)*                 Kleene: kmax
//! ke         := tres ('&' tres)*             Kleene: kmin
//! tres       := soma ('<=>' soma)?           cmp → trit
//! soma       := termo (('+'|'-') termo)*
//! termo      := unário (('*'|'/'|'%') unário)*
//! unário     := ('-'|'!') unário | primário
//! primário   := número | 'true' | 'false' | ident | ident '[' expr ']' | chamada | '(' expr ')'
//! chamada    := ident '(' (expr (',' expr)*)? ')'
//! ```
//!
//! Sem `->` a função não devolve valor (docs/18 §6). Tipos: docs/18 §4.2.

use super::lex::{Tok, Token};

/// Tipos da superfície (docs/18 §4.2). Todos ocupam uma palavra; no TIR
/// viram `word`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Type {
    Word,
    Bool,
    Trit,
}

impl Type {
    pub fn name(self) -> &'static str {
        match self {
            Type::Word => "word",
            Type::Bool => "bool",
            Type::Trit => "trit",
        }
    }
}

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
    /// `&`: mínimo trit a trit (∧ de Kleene; E sem desvio em bool)
    Kmin,
    /// `|`: máximo trit a trit (∨ de Kleene)
    Kmax,
    /// `<=>`: sinal de a − b, um trit
    Cmp,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Not,
}

#[derive(Debug)]
pub enum Expr {
    Num(i64),
    Bool(bool),
    Var(String, usize, usize),
    Call(String, Vec<Expr>, usize, usize),
    /// `nome[i]`: só sobre `static` de array
    Index(String, Box<Expr>, usize, usize),
    /// operador, operando, linha e coluna do operador
    Unary(UnOp, Box<Expr>, usize, usize),
    Binary(BinOp, Box<Expr>, Box<Expr>, usize, usize),
}

#[derive(Debug)]
pub enum Stmt {
    Let { name: String, mutable: bool, ty: Option<Type>, init: Expr, line: usize, col: usize },
    Assign { name: String, value: Expr, line: usize, col: usize },
    Store { name: String, index: Expr, value: Expr, line: usize, col: usize },
    If { cond: Expr, then: Block, els: Option<Block>, line: usize, col: usize },
    While { cond: Expr, body: Block, line: usize, col: usize },
    Return { value: Option<Expr>, line: usize, col: usize },
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
    pub ptys: Vec<Type>,
    /// `None` = sem valor de retorno
    pub ret: Option<Type>,
    pub body: Block,
    pub line: usize,
    pub col: usize,
}

/// `static mut nome: word;` ou `static mut nome: [word; n];` — dado
/// definido FORA do programa (p.ex. no `.data` do núcleo), docs/15.
#[derive(Debug)]
pub struct Static {
    pub name: String,
    pub size: i64,
    pub array: bool,
    pub line: usize,
    pub col: usize,
}

#[derive(Debug)]
pub struct Program {
    pub fns: Vec<Fn>,
    pub statics: Vec<Static>,
}

pub struct ParseError {
    pub line: usize,
    pub col: usize,
    pub msg: String,
}

/// Recursão do analisador: blocos, parênteses e unários aninhados. Cada
/// bloco vira uma região do TIR, então fica bem abaixo de `ir::MAX_DEPTH`.
pub const MAX_NEST: usize = 256;
/// Profundidade de uma árvore de expressão. Cadeias `a + b + …` crescem
/// sem recursão aqui, mas os passes seguintes recursam nelas.
pub const MAX_EXPR: usize = 1024;

pub struct Parser {
    toks: Vec<Token>,
    i: usize,
    /// recursão corrente (≤ MAX_NEST)
    nest: usize,
    /// profundidade da última expressão devolvida (≤ MAX_EXPR)
    depth: usize,
}

type R<T> = Result<T, ParseError>;

impl Parser {
    pub fn new(toks: Vec<Token>) -> Self {
        Parser { toks, i: 0, nest: 0, depth: 0 }
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

    /// Acima dos limites o t27r morria por estouro de pilha (SIGABRT).
    fn enter(&mut self) -> R<()> {
        self.nest += 1;
        if self.nest > MAX_NEST {
            return self.err(format!("aninhamento além de {MAX_NEST} níveis (blocos, parênteses, unários)"));
        }
        Ok(())
    }

    fn deeper(&mut self, depth: usize) -> R<()> {
        self.depth = depth;
        if depth > MAX_EXPR {
            return self.err(format!("aninhamento de expressão além de {MAX_EXPR} níveis"));
        }
        Ok(())
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
            Tok::Reserved(s) => self.err(format!("'{s}' é palavra reservada (docs/18 §3)")),
            other => self.err(format!("esperava um nome, achei {other:?}")),
        }
    }

    fn ty(&mut self) -> R<Type> {
        let t = match self.peek() {
            Tok::Word => Type::Word,
            Tok::Bool => Type::Bool,
            Tok::Trit => Type::Trit,
            other => return self.err(format!("esperava um tipo (word, bool, trit), achei {other:?}")),
        };
        self.bump();
        Ok(t)
    }

    pub fn program(&mut self) -> R<Program> {
        let (mut fns, mut statics) = (Vec::new(), Vec::new());
        while self.peek() != &Tok::Eof {
            if self.peek() == &Tok::Static {
                statics.push(self.static_item()?);
            } else {
                fns.push(self.function()?);
            }
        }
        Ok(Program { fns, statics })
    }

    fn static_item(&mut self) -> R<Static> {
        let (line, col) = self.pos();
        self.bump();
        self.expect(&Tok::Mut, "'mut' (dado externo é sempre mutável)")?;
        let name = self.ident()?;
        self.expect(&Tok::Colon, "':'")?;
        let (size, array) = if self.eat(&Tok::LBracket) {
            self.expect(&Tok::Word, "o tipo 'word'")?;
            self.expect(&Tok::Semi, "';'")?;
            let n = match self.peek().clone() {
                Tok::Num(v) => v,
                other => return self.err(format!("esperava o tamanho do array, achei {other:?}")),
            };
            self.bump();
            self.expect(&Tok::RBracket, "']'")?;
            (n, true)
        } else {
            self.expect(&Tok::Word, "o tipo 'word' ou '[word; n]'")?;
            (1, false)
        };
        self.expect(&Tok::Semi, "';'")?;
        Ok(Static { name, size, array, line, col })
    }

    fn function(&mut self) -> R<Fn> {
        let (line, col) = self.pos();
        self.expect(&Tok::Fn, "'fn'")?;
        let name = self.ident()?;
        self.expect(&Tok::LParen, "'('")?;
        let (mut params, mut ptys) = (Vec::new(), Vec::new());
        if self.peek() != &Tok::RParen {
            loop {
                params.push(self.ident()?);
                self.expect(&Tok::Colon, "':'")?;
                ptys.push(self.ty()?);
                if !self.eat(&Tok::Comma) {
                    break;
                }
            }
        }
        self.expect(&Tok::RParen, "')'")?;
        let ret = if self.eat(&Tok::Arrow) { Some(self.ty()?) } else { None };
        let body = self.block()?;
        Ok(Fn { name, params, ptys, ret, body, line, col })
    }

    fn block(&mut self) -> R<Block> {
        self.expect(&Tok::LBrace, "'{'")?;
        self.enter()?;
        let mut stmts = Vec::new();
        while self.peek() != &Tok::RBrace {
            if self.peek() == &Tok::Eof {
                return self.err("bloco sem '}'");
            }
            stmts.push(self.stmt()?);
        }
        self.expect(&Tok::RBrace, "'}'")?;
        self.nest -= 1;
        Ok(Block { stmts })
    }

    fn stmt(&mut self) -> R<Stmt> {
        match self.peek().clone() {
            Tok::Let => {
                let (line, col) = self.pos();
                self.bump();
                let mutable = self.eat(&Tok::Mut);
                let name = self.ident()?;
                let ty = if self.eat(&Tok::Colon) { Some(self.ty()?) } else { None };
                self.expect(&Tok::Assign, "'='")?;
                let init = self.expr()?;
                self.expect(&Tok::Semi, "';'")?;
                Ok(Stmt::Let { name, mutable, ty, init, line, col })
            }
            Tok::If => {
                let (line, col) = self.pos();
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
                Ok(Stmt::If { cond, then, els, line, col })
            }
            Tok::While => {
                let (line, col) = self.pos();
                self.bump();
                let cond = self.expr()?;
                let body = self.block()?;
                Ok(Stmt::While { cond, body, line, col })
            }
            Tok::Return => {
                let (line, col) = self.pos();
                self.bump();
                let value = if self.peek() == &Tok::Semi { None } else { Some(self.expr()?) };
                self.expect(&Tok::Semi, "';'")?;
                Ok(Stmt::Return { value, line, col })
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
            Tok::Ident(name) if self.peek_at(1) == &Tok::LBracket => {
                // `a[i] = v;` — ou uma expressão que começa com `a[i]`
                let (line, col) = self.pos();
                let save = self.i;
                self.bump();
                self.bump();
                let index = self.expr()?;
                self.expect(&Tok::RBracket, "']'")?;
                if self.eat(&Tok::Assign) {
                    let value = self.expr()?;
                    self.expect(&Tok::Semi, "';'")?;
                    return Ok(Stmt::Store { name, index, value, line, col });
                }
                self.i = save;
                let e = self.expr()?;
                self.expect(&Tok::Semi, "';'")?;
                Ok(Stmt::Expr(e))
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
        self.enter()?;
        let e = self.or()?;
        self.nest -= 1;
        Ok(e)
    }

    /// `operando (op operando)*` com operadores associativos à esquerda
    fn left(&mut self, ops: &[(Tok, BinOp)], next: fn(&mut Self) -> R<Expr>) -> R<Expr> {
        let mut e = next(self)?;
        while let Some(&(_, op)) = ops.iter().find(|(t, _)| t == self.peek()) {
            let (line, col) = self.pos();
            let d = self.depth;
            self.bump();
            let r = next(self)?;
            self.deeper(d.max(self.depth) + 1)?;
            e = Expr::Binary(op, Box::new(e), Box::new(r), line, col);
        }
        Ok(e)
    }

    fn or(&mut self) -> R<Expr> {
        self.left(&[(Tok::OrOr, BinOp::Or)], Self::and)
    }

    fn and(&mut self) -> R<Expr> {
        self.left(&[(Tok::AndAnd, BinOp::And)], Self::cmp)
    }

    fn kor(&mut self) -> R<Expr> {
        self.left(&[(Tok::Pipe, BinOp::Kmax)], Self::kand)
    }

    fn kand(&mut self) -> R<Expr> {
        self.left(&[(Tok::Amp, BinOp::Kmin)], Self::ship)
    }

    /// `a <=> b` não se encadeia: `a <=> b <=> c` não tem leitura útil
    fn ship(&mut self) -> R<Expr> {
        let e = self.sum()?;
        if self.peek() != &Tok::Ship {
            return Ok(e);
        }
        let (line, col) = self.pos();
        let d = self.depth;
        self.bump();
        let r = self.sum()?;
        self.deeper(d.max(self.depth) + 1)?;
        Ok(Expr::Binary(BinOp::Cmp, Box::new(e), Box::new(r), line, col))
    }

    fn cmp(&mut self) -> R<Expr> {
        use BinOp::*;
        let ops = [(Tok::EqEq, Eq), (Tok::Ne, Ne), (Tok::Lt, Lt), (Tok::Le, Le), (Tok::Gt, Gt), (Tok::Ge, Ge)];
        self.left(&ops, Self::kor)
    }

    fn sum(&mut self) -> R<Expr> {
        self.left(&[(Tok::Plus, BinOp::Add), (Tok::Minus, BinOp::Sub)], Self::term)
    }

    fn term(&mut self) -> R<Expr> {
        self.left(&[(Tok::Star, BinOp::Mul), (Tok::Slash, BinOp::Div), (Tok::Percent, BinOp::Rem)], Self::unary)
    }

    fn unary(&mut self) -> R<Expr> {
        let (line, col) = self.pos();
        let op = match self.peek() {
            Tok::Minus => UnOp::Neg,
            Tok::Bang => UnOp::Not,
            _ => return self.primary(),
        };
        self.bump();
        self.enter()?;
        let e = self.unary()?;
        self.nest -= 1;
        self.deeper(self.depth + 1)?;
        Ok(Expr::Unary(op, Box::new(e), line, col))
    }

    fn primary(&mut self) -> R<Expr> {
        match self.peek().clone() {
            Tok::Num(v) => {
                self.bump();
                self.depth = 1;
                Ok(Expr::Num(v))
            }
            Tok::True => {
                self.bump();
                self.depth = 1;
                Ok(Expr::Bool(true))
            }
            Tok::False => {
                self.bump();
                self.depth = 1;
                Ok(Expr::Bool(false))
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
                    let (mut args, mut d) = (Vec::new(), 0);
                    if self.peek() != &Tok::RParen {
                        loop {
                            args.push(self.expr()?);
                            d = d.max(self.depth);
                            if !self.eat(&Tok::Comma) {
                                break;
                            }
                        }
                    }
                    self.expect(&Tok::RParen, "')'")?;
                    self.deeper(d + 1)?;
                    Ok(Expr::Call(name, args, line, col))
                } else if self.eat(&Tok::LBracket) {
                    let i = self.expr()?;
                    self.expect(&Tok::RBracket, "']'")?;
                    self.deeper(self.depth + 1)?;
                    Ok(Expr::Index(name, Box::new(i), line, col))
                } else {
                    self.depth = 1;
                    Ok(Expr::Var(name, line, col))
                }
            }
            Tok::Reserved(s) => self.err(format!("'{s}' é palavra reservada (docs/18 §3)")),
            other => self.err(format!("esperava uma expressão, achei {other:?}")),
        }
    }
}
