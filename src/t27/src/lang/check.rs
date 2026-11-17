//! TENERURA — checagem estática do subconjunto.
//!
//! O que é verificado antes de gerar código:
//! - nomes de função únicos; em programa de usuário, `main` existe e não
//!   tem parâmetros (módulo do núcleo é biblioteca: não tem `main`);
//! - parâmetros únicos; chamadas a funções existentes com a aridade certa;
//! - variável usada só depois de declarada (e no escopo certo);
//! - atribuição só em `let mut` ou `static mut` (o "sabor Rust" mínimo);
//! - `static` com tamanho em 1..3⁹, nome único; array só com `a[i]`,
//!   escalar só pelo nome; índice literal fora do array é erro;
//! - embutidas do modo certo: `putc`/`exit`/IPC só em usuário,
//!   `csr_*`/`phys_*`/`wfi`/`tlbi` só no núcleo (docs/16); CSR literal,
//!   existente e, para escrita, gravável;
//! - `match` exaustivo (exige `_`) e sem padrão repetido;
//! - literais dentro da palavra de 27 trits;
//! - `exit` não retorna: só pode ser usado como comando.

use std::collections::{HashMap, HashSet};

use super::ir::{csr_writable, Mode};
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

/// Aridade das funções embutidas e o modo em que existem (`None` = ambos).
pub fn builtin(name: &str) -> Option<(usize, Option<Mode>)> {
    let u = Some(Mode::User);
    let k = Some(Mode::Priv);
    Some(match name {
        "putc" => (1, u),     // putc(caractere)
        "putdec" => (1, u),   // putdec(valor)
        "exit" => (1, u),     // exit(código) — não retorna
        "yield" => (0, u),    // yield()
        "time" => (0, u),     // time() -> relógio modular
        "call" => (3, u),     // call(cap, op, valor) -> primeira palavra da resposta
        "recv" => (2, u),     // recv(cap, paciência) -> estado
        "recv_val" => (2, u), // recv_val(cap, paciência) -> a3 da mensagem (ou estado)
        "sign" => (1, None),  // sign(x) -> −1 / 0 / +1 (o trit do ternário)
        "shift" => (2, None), // shift(x, k): ×3ᵏ (k > 0) ou descarta |k| trits (k < 0)
        "csr_read" => (1, k), // csr_read(CSR) -> valor
        "csr_write" => (2, k),
        "csr_swap" => (2, k), // troca atômica (csrx) -> valor antigo
        "wfi" => (0, k),
        "tlbi" => (0, k),
        "phys_read" => (1, k), // endereço físico (MMIO ou quadro) -> palavra
        "phys_write" => (2, k),
        _ => return None,
    })
}

/// Compatibilidade: aridade de uma embutida, em qualquer modo.
pub fn builtin_arity(name: &str) -> Option<usize> {
    builtin(name).map(|b| b.0)
}

struct Ck<'a> {
    mode: Mode,
    sigs: HashMap<&'a str, usize>,
    /// nome → (tamanho, é array)
    statics: HashMap<&'a str, (i64, bool)>,
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

    /// `nome[i]`: só sobre static de array não sombreado por local
    fn array(&mut self, name: &str, index: &Expr, line: usize, col: usize) -> R<()> {
        match (self.lookup(name), self.statics.get(name)) {
            (None, Some(&(size, true))) => {
                if let Expr::Num(v) = index {
                    if *v < 0 || *v >= size {
                        return d(line, col, format!("índice {v} fora de '{name}' ([word; {size}])"));
                    }
                }
                self.expr(index)
            }
            (None, None) => d(line, col, format!("'{name}' não existe")),
            _ => d(line, col, format!("'{name}' não é array")),
        }
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
                match (self.lookup(name), self.statics.get(name.as_str())) {
                    (Some(true), _) | (None, Some((_, false))) => Ok(()),
                    (Some(false), _) => d(
                        *line,
                        *col,
                        format!("'{name}' não é mutável: declare com 'let mut {name}'"),
                    ),
                    (None, Some((_, true))) => d(*line, *col, format!("'{name}' é array: use '{name}[i] = ...'")),
                    (None, None) => d(*line, *col, format!("'{name}' não existe")),
                }
            }
            Stmt::Store { name, index, value, line, col } => {
                self.expr(value)?;
                self.array(name, index, *line, *col)
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
                if let Expr::Call(name, args, line, col) = e {
                    if name == "exit" {
                        if self.mode != Mode::User {
                            return d(*line, *col, "'exit' encerra uma tarefa: não existe no núcleo");
                        }
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
            Expr::Var(name, line, col) => match (self.lookup(name), self.statics.get(name.as_str())) {
                (Some(_), _) | (None, Some((_, false))) => Ok(()),
                (None, Some((_, true))) => d(*line, *col, format!("'{name}' é array: use '{name}[i]'")),
                (None, None) => d(*line, *col, format!("'{name}' não existe")),
            },
            Expr::Index(name, i, line, col) => self.array(name, i, *line, *col),
            Expr::Call(name, args, line, col) => {
                if name == "exit" {
                    return d(*line, *col, "exit não retorna: use como comando, não como valor");
                }
                let want = match builtin(name) {
                    Some((n, mode)) => {
                        if mode.is_some_and(|m| m != self.mode) {
                            let onde = if self.mode == Mode::User { "em programa de usuário" } else { "no núcleo" };
                            return d(*line, *col, format!("'{name}' não existe {onde} (docs/16)"));
                        }
                        n
                    }
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
                if name.starts_with("csr_") {
                    // o CSR é parte da instrução, não um valor: literal obrigatório
                    let Expr::Num(c) = &args[0] else {
                        return d(*line, *col, format!("'{name}': o CSR tem de ser um número literal (1..12)"));
                    };
                    let ok = if name == "csr_read" { (1..=12).contains(c) } else { csr_writable(*c as u32) && *c > 0 };
                    if !ok {
                        return d(*line, *col, format!("'{name}': CSR {c} não existe ou não aceita escrita"));
                    }
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

pub fn check(p: &Program, mode: Mode) -> R<()> {
    let mut sigs = HashMap::new();
    for f in &p.fns {
        if builtin(&f.name).is_some() {
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
    let mut statics = HashMap::new();
    for s in &p.statics {
        if builtin(&s.name).is_some() || sigs.contains_key(s.name.as_str()) || statics.contains_key(s.name.as_str()) {
            return d(s.line, s.col, format!("'{}' já é nome de função, embutida ou static", s.name));
        }
        if s.size < 1 || s.size > super::ir::SLOT_MAX as i64 {
            return d(s.line, s.col, format!("'{}': tamanho {} fora de 1..3⁹", s.name, s.size));
        }
        statics.insert(s.name.as_str(), (s.size, s.array));
    }
    if mode == Mode::User {
        match sigs.get("main") {
            None => return d(1, 1, "falta 'fn main() -> word'"),
            Some(0) => {}
            Some(_) => return d(1, 1, "main não pode ter parâmetros"),
        }
    }
    let mut ck = Ck { mode, sigs, statics, scopes: Vec::new() };
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
