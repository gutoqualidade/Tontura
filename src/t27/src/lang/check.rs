//! TENERURA — checagem estática do subconjunto.
//!
//! O que é verificado antes de gerar código:
//! - nomes de função únicos; `main` existe e não tem parâmetros;
//! - parâmetros únicos; chamadas a funções existentes com a aridade certa;
//! - variável usada só depois de declarada (e no escopo certo);
//! - atribuição só em `let mut` (é o "sabor Rust" mínimo do subconjunto);
//! - `match` exaustivo (exige `_`) e sem padrão repetido;
//! - literais dentro da palavra de 27 trits;
//! - `exit` não retorna: só pode ser usado como comando.

use std::collections::{HashMap, HashSet};

use super::parse::{Arm, Block, Expr, Fn, Pat, Program, Stmt, UnOp};
use crate::word::M27;

pub struct Diag {
    pub line: usize,
    pub col: usize,
    pub msg: String,
}

type R<T> = Result<T, Diag>;

fn d<T>(line: usize, col: usize, msg: impl Into<String>) -> R<T> {
    Err(Diag { line, col, msg: msg.into() })
}

/// Aridade das funções embutidas (as que viram instruções ou rotinas da
/// biblioteca de usuário).
pub fn builtin_arity(name: &str) -> Option<usize> {
    Some(match name {
        "putc" => 1,    // putc(caractere)
        "putdec" => 1,  // putdec(valor)
        "exit" => 1,    // exit(código) — não retorna
        "yield" => 0,   // yield()
        "time" => 0,    // time() -> relógio modular
        "sign" => 1,    // sign(x) -> −1 / 0 / +1 (o trit do ternário)
        "call" => 3,    // call(cap, op, valor) -> primeira palavra da resposta
        "recv" => 2,    // recv(cap, paciência) -> estado
        "recv_val" => 2, // recv_val(cap, paciência) -> a3 da mensagem (ou estado)
        _ => return None,
    })
}

struct Ck<'a> {
    sigs: HashMap<&'a str, usize>,
    scopes: Vec<HashMap<String, bool>>,
}

impl<'a> Ck<'a> {
    fn declare(&mut self, name: &str, mutable: bool, line: usize, col: usize) -> R<()> {
        let scope = self.scopes.last_mut().unwrap();
        if scope.contains_key(name) {
            return d(line, col, format!("'{name}' já foi declarada neste escopo"));
        }
        scope.insert(name.to_string(), mutable);
        Ok(())
    }

    fn lookup(&self, name: &str) -> Option<bool> {
        self.scopes.iter().rev().find_map(|s| s.get(name).copied())
    }

    fn block(&mut self, b: &Block) -> R<()> {
        self.scopes.push(HashMap::new());
        for s in &b.stmts {
            self.stmt(s)?;
        }
        self.scopes.pop();
        Ok(())
    }

    fn stmt(&mut self, s: &Stmt) -> R<()> {
        match s {
            Stmt::Let { name, mutable, init, line, col } => {
                self.expr(init)?;
                self.declare(name, *mutable, *line, *col)
            }
            Stmt::Assign { name, value, line, col } => {
                self.expr(value)?;
                match self.lookup(name) {
                    None => d(*line, *col, format!("'{name}' não existe")),
                    Some(false) => d(
                        *line,
                        *col,
                        format!("'{name}' não é mutável: declare com 'let mut {name}'"),
                    ),
                    Some(true) => Ok(()),
                }
            }
            Stmt::If { cond, then, els } => {
                self.expr(cond)?;
                self.block(then)?;
                if let Some(e) = els {
                    self.block(e)?;
                }
                Ok(())
            }
            Stmt::While { cond, body } => {
                self.expr(cond)?;
                self.block(body)
            }
            Stmt::Return { value } => match value {
                Some(e) => self.expr(e),
                None => Ok(()),
            },
            Stmt::Match { scrutinee, arms, line } => {
                self.expr(scrutinee)?;
                self.match_arms(arms, *line)
            }
            Stmt::Expr(e) => {
                // `exit(...)` é comando: não retorna, então não pode ser valor
                if let Expr::Call(name, args, _, _) = e {
                    if name == "exit" {
                        if args.len() != 1 {
                            return d(0, 0, "exit espera 1 argumento");
                        }
                        return self.expr(&args[0]);
                    }
                }
                self.expr(e)
            }
        }
    }

    fn match_arms(&mut self, arms: &[Arm], line: usize) -> R<()> {
        let mut lits: HashSet<i64> = HashSet::new();
        let mut has_wild = false;
        for arm in arms {
            for p in &arm.pats {
                match p {
                    Pat::Wild => {
                        if has_wild {
                            return d(line, 0, "mais de um padrão '_' no mesmo match");
                        }
                        has_wild = true;
                    }
                    Pat::Lit(v) => {
                        if !lits.insert(*v) {
                            return d(line, 0, format!("padrão {v} repetido no match"));
                        }
                    }
                }
            }
            self.block(&arm.body)?;
        }
        if !has_wild {
            return d(
                line,
                0,
                "match sem '_': sobre uma palavra de 27 trits não há como enumerar todos os casos",
            );
        }
        Ok(())
    }

    fn expr(&mut self, e: &Expr) -> R<()> {
        match e {
            Expr::Num(v) => {
                if *v < -M27 || *v > M27 {
                    return d(0, 0, format!("literal {v} não cabe em 27 trits (|v| ≤ {M27})"));
                }
                Ok(())
            }
            Expr::Var(name, line, col) => match self.lookup(name) {
                Some(_) => Ok(()),
                None => d(*line, *col, format!("'{name}' não existe")),
            },
            Expr::Call(name, args, line, col) => {
                if name == "exit" {
                    return d(*line, *col, "exit não retorna: use como comando, não como valor");
                }
                let want = match builtin_arity(name) {
                    Some(n) => n,
                    None => match self.sigs.get(name.as_str()) {
                        Some(n) => *n,
                        None => return d(*line, *col, format!("função '{name}' não existe")),
                    },
                };
                if args.len() != want {
                    return d(
                        *line,
                        *col,
                        format!("'{name}' espera {want} argumento(s), recebeu {}", args.len()),
                    );
                }
                if want > 9 {
                    return d(*line, *col, format!("'{name}': no máximo 9 argumentos (a1..a9)"));
                }
                for a in args {
                    self.expr(a)?;
                }
                Ok(())
            }
            Expr::Unary(UnOp::Neg, x) => self.expr(x),
            Expr::Unary(UnOp::Not, x) => self.expr(x),
            Expr::Binary(_, a, b) => {
                self.expr(a)?;
                self.expr(b)
            }
        }
    }
}

pub fn check(p: &Program) -> R<()> {
    let mut sigs = HashMap::new();
    for f in &p.fns {
        if builtin_arity(&f.name).is_some() {
            return d(f.line, f.col, format!("'{}' é embutida e não pode ser redefinida", f.name));
        }
        if sigs.insert(f.name.as_str(), f.params.len()).is_some() {
            return d(f.line, f.col, format!("função '{}' definida duas vezes", f.name));
        }
        let mut seen = HashSet::new();
        for prm in &f.params {
            if !seen.insert(prm.as_str()) {
                return d(f.line, f.col, format!("parâmetro '{prm}' repetido em '{}'", f.name));
            }
        }
        if f.params.len() > 9 {
            return d(f.line, f.col, format!("'{}': no máximo 9 parâmetros (a1..a9)", f.name));
        }
    }
    match sigs.get("main") {
        None => return d(1, 1, "falta 'fn main() -> word'"),
        Some(0) => {}
        Some(_) => return d(1, 1, "main não pode ter parâmetros"),
    }
    let mut ck = Ck { sigs, scopes: Vec::new() };
    for f in &p.fns {
        ck.fn_body(f)?;
    }
    Ok(())
}

impl<'a> Ck<'a> {
    fn fn_body(&mut self, f: &Fn) -> R<()> {
        self.scopes.push(HashMap::new());
        for p in &f.params {
            // parâmetros são imutáveis, como em Rust sem `mut`
            self.declare(p, false, f.line, f.col)?;
        }
        for s in &f.body.stmts {
            self.stmt(s)?;
        }
        self.scopes.pop();
        Ok(())
    }
}
