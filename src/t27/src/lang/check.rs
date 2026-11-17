//! TENERURA — checagem estática do T27R, com tipos (docs/18 §4.2).
//!
//! O que é verificado antes de gerar código:
//! - nomes de função únicos; em programa de usuário, `main` existe, não
//!   tem parâmetros e devolve `word` (o código de saída); módulo do núcleo
//!   é biblioteca: não tem `main`;
//! - parâmetros únicos; chamadas a funções existentes com a aridade e os
//!   tipos certos;
//! - tipos: `word`, `bool`, `trit` são distintos. Só `trit → word` é
//!   implícito (alargar não muda o valor); literal −1/0/1 serve onde se
//!   espera `trit`. `if`/`while` só aceitam `bool`; aritmética e ordem só
//!   números; `&`/`|` (Kleene) só `trit` ou só `bool`; `<=>` dá `trit`;
//! - função sem `->` não devolve valor: chamá-la não é expressão;
//! - variável usada só depois de declarada (e no escopo certo);
//! - atribuição só em `let mut` ou `static mut` (o "sabor Rust" mínimo);
//! - `static` com tamanho em 1..3⁹, nome único; array só com `a[i]`,
//!   escalar só pelo nome; índice literal fora do array é erro;
//! - embutidas do modo certo: `putc`/`exit`/IPC só em usuário,
//!   `csr_*`/`phys_*`/`wfi`/`tlbi` só no núcleo (docs/16); CSR literal,
//!   existente e, para escrita, gravável;
//! - `match` exaustivo: sobre `trit`, os três valores ou `_`; sobre `word`,
//!   `_` obrigatório; sem padrão repetido nem impossível;
//! - literais dentro da palavra de 27 trits;
//! - `exit` não retorna: só pode ser usado como comando.

use std::collections::{HashMap, HashSet};

use super::ir::{csr_writable, Mode};
use super::parse::{Arm, BinOp, Block, Expr, Fn, Pat, Program, Stmt, Type, UnOp};
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

/// Tipo do resultado de uma embutida (`None` = não devolve valor). Todos
/// os argumentos são `word`.
fn builtin_ret(name: &str) -> Option<Type> {
    match name {
        "putc" | "putdec" | "exit" | "csr_write" | "wfi" | "tlbi" | "phys_write" => None,
        "sign" => Some(Type::Trit),
        _ => Some(Type::Word),
    }
}

/// Tipo de uma expressão; `None` = não produz valor (chamada sem `->`).
type V = Option<Type>;

fn tname(v: V) -> &'static str {
    v.map_or("nada (função sem '->')", Type::name)
}

/// `v` serve onde se espera `want`? Só alargar `trit → word` é implícito.
fn fits(v: V, want: Type) -> bool {
    v == Some(want) || (v == Some(Type::Trit) && want == Type::Word)
}

fn numeric(v: V) -> bool {
    matches!(v, Some(Type::Word | Type::Trit))
}

/// Posição de uma expressão para mensagens (literais não guardam posição).
fn pos(e: &Expr) -> (usize, usize) {
    match e {
        Expr::Var(_, l, c) | Expr::Call(_, _, l, c) | Expr::Index(_, _, l, c) => (*l, *c),
        Expr::Unary(_, _, l, c) | Expr::Binary(_, _, _, l, c) => (*l, *c),
        Expr::Num(_) | Expr::Bool(_) => (0, 0),
    }
}

struct Ck<'a> {
    mode: Mode,
    sigs: HashMap<&'a str, (&'a [Type], Option<Type>)>,
    /// nome → (tamanho, é array); todo static é `word`
    statics: HashMap<&'a str, (i64, bool)>,
    /// nome → (mutável, tipo)
    scopes: Vec<HashMap<String, (bool, Type)>>,
    /// tipo de retorno da função corrente
    ret: Option<Type>,
}

impl<'a> Ck<'a> {
    fn declare(&mut self, name: &str, mutable: bool, ty: Type, line: usize, col: usize) -> R<()> {
        let scope = self.scopes.last_mut().unwrap();
        if scope.contains_key(name) {
            return d(line, col, format!("'{name}' já foi declarada neste escopo"));
        }
        scope.insert(name.to_string(), (mutable, ty));
        Ok(())
    }

    fn lookup(&self, name: &str) -> Option<(bool, Type)> {
        self.scopes.iter().rev().find_map(|s| s.get(name).copied())
    }

    /// Confere `e` contra o tipo esperado `want`.
    fn expect(&mut self, e: &Expr, want: Type, what: &str) -> R<()> {
        let v = self.expr(e, Some(want))?;
        if !fits(v, want) {
            let (l, c) = pos(e);
            return d(l, c, format!("{what}: esperava {}, achei {}", want.name(), tname(v)));
        }
        Ok(())
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
                self.expect(index, Type::Word, "índice")
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

    fn cond(&mut self, e: &Expr, line: usize, col: usize) -> R<()> {
        let v = self.expr(e, Some(Type::Bool))?;
        if v != Some(Type::Bool) {
            let hint = if numeric(v) { ": escreva a comparação, por exemplo 'x != 0'" } else { "" };
            return d(line, col, format!("a condição tem de ser bool, achei {}{hint}", tname(v)));
        }
        Ok(())
    }

    fn stmt(&mut self, s: &Stmt) -> R<()> {
        match s {
            Stmt::Let { name, mutable, ty, init, line, col } => {
                let t = match ty {
                    Some(t) => {
                        self.expect(init, *t, &format!("'{name}'"))?;
                        *t
                    }
                    None => match self.expr(init, None)? {
                        Some(t) => t,
                        None => return d(*line, *col, format!("'{name}': a expressão não produz valor")),
                    },
                };
                self.declare(name, *mutable, t, *line, *col)
            }
            Stmt::Assign { name, value, line, col } => {
                let t = match (self.lookup(name), self.statics.get(name.as_str())) {
                    (Some((true, t)), _) => t,
                    (None, Some((_, false))) => Type::Word,
                    (Some((false, _)), _) => {
                        return d(*line, *col, format!("'{name}' não é mutável: declare com 'let mut {name}'"))
                    }
                    (None, Some((_, true))) => return d(*line, *col, format!("'{name}' é array: use '{name}[i] = ...'")),
                    (None, None) => return d(*line, *col, format!("'{name}' não existe")),
                };
                self.expect(value, t, &format!("'{name}'"))
            }
            Stmt::Store { name, index, value, line, col } => {
                self.expect(value, Type::Word, &format!("'{name}[...]'"))?;
                self.array(name, index, *line, *col)
            }
            Stmt::If { cond, then, els, line, col } => {
                self.cond(cond, *line, *col)?;
                self.block(then)?;
                if let Some(e) = els {
                    self.block(e)?;
                }
                Ok(())
            }
            Stmt::While { cond, body, line, col } => {
                self.cond(cond, *line, *col)?;
                self.block(body)
            }
            Stmt::Return { value, line, col } => match (value, self.ret) {
                (Some(e), Some(t)) => self.expect(e, t, "valor de retorno"),
                (None, None) => Ok(()),
                (Some(_), None) => d(*line, *col, "a função não tem '->': 'return' sem valor"),
                (None, Some(t)) => d(*line, *col, format!("falta o valor de retorno ({})", t.name())),
            },
            Stmt::Match { scrutinee, arms, line } => {
                let v = self.expr(scrutinee, None)?;
                match v {
                    Some(Type::Word | Type::Trit) => self.match_arms(arms, v == Some(Type::Trit), *line),
                    _ => d(*line, 0, format!("match sobre {}: só sobre word ou trit (bool: use if)", tname(v))),
                }
            }
            Stmt::Expr(e) => {
                // `exit(...)` é comando: não retorna, então não pode ser valor
                if let Expr::Call(name, args, line, col) = e {
                    if name == "exit" {
                        if self.mode != Mode::User {
                            return d(*line, *col, "'exit' encerra uma tarefa: não existe no núcleo");
                        }
                        if args.len() != 1 {
                            return d(*line, *col, "exit espera 1 argumento");
                        }
                        return self.expect(&args[0], Type::Word, "exit");
                    }
                }
                self.expr(e, None).map(|_| ())
            }
        }
    }

    fn match_arms(&mut self, arms: &[Arm], trit: bool, line: usize) -> R<()> {
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
                        if trit && !(-1..=1).contains(v) {
                            return d(line, 0, format!("padrão {v} impossível: um trit só vale −1, 0 ou 1"));
                        }
                        if !lits.insert(*v) {
                            return d(line, 0, format!("padrão {v} repetido no match"));
                        }
                    }
                }
            }
            self.block(&arm.body)?;
        }
        if has_wild || (trit && lits.len() == 3) {
            return Ok(());
        }
        if trit {
            return d(line, 0, "match sobre trit não exaustivo: cubra −1, 0 e 1 (ou use '_')");
        }
        d(line, 0, "match sem '_': sobre uma palavra de 27 trits não há como enumerar todos os casos")
    }

    /// Literal −1, 0 ou 1 (com ou sem sinal): serve como `trit`.
    fn trit_lit(e: &Expr) -> bool {
        match e {
            Expr::Num(v) => (-1..=1).contains(v),
            Expr::Unary(UnOp::Neg, x, ..) => matches!(**x, Expr::Num(1)),
            _ => false,
        }
    }

    /// Tipo de `e`; `want` só orienta literais (−1/0/1 viram `trit`).
    fn expr(&mut self, e: &Expr, want: V) -> R<V> {
        match e {
            Expr::Num(v) => {
                if *v < -M27 || *v > M27 {
                    return d(0, 0, format!("literal {v} não cabe em 27 trits (|v| ≤ {M27})"));
                }
                Ok(Some(if want == Some(Type::Trit) && (-1..=1).contains(v) { Type::Trit } else { Type::Word }))
            }
            Expr::Bool(_) => Ok(Some(Type::Bool)),
            Expr::Var(name, line, col) => match (self.lookup(name), self.statics.get(name.as_str())) {
                (Some((_, t)), _) => Ok(Some(t)),
                (None, Some((_, false))) => Ok(Some(Type::Word)),
                (None, Some((_, true))) => d(*line, *col, format!("'{name}' é array: use '{name}[i]'")),
                (None, None) => d(*line, *col, format!("'{name}' não existe")),
            },
            Expr::Index(name, i, line, col) => self.array(name, i, *line, *col).map(|_| Some(Type::Word)),
            Expr::Call(name, args, line, col) => self.call(name, args, *line, *col),
            Expr::Unary(op, x, line, col) => {
                let v = self.expr(x, want)?;
                match (op, v) {
                    (UnOp::Neg, Some(Type::Word | Type::Trit)) => Ok(v),
                    (UnOp::Not, Some(Type::Bool)) => Ok(v),
                    (UnOp::Neg, _) => d(*line, *col, format!("'-' sobre {}: só sobre word ou trit", tname(v))),
                    (UnOp::Not, _) => d(
                        *line,
                        *col,
                        format!("'!' sobre {}: só sobre bool (palavra: 'x == 0'; trit: '-t')", tname(v)),
                    ),
                }
            }
            Expr::Binary(op, a, b, line, col) => {
                let (line, col) = (*line, *col);
                match op {
                    BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Rem | BinOp::Cmp => {
                        let (ta, tb) = (self.expr(a, None)?, self.expr(b, None)?);
                        if !numeric(ta) || !numeric(tb) {
                            return d(line, col, format!("aritmética sobre {} e {}: só números", tname(ta), tname(tb)));
                        }
                        Ok(Some(if *op == BinOp::Cmp { Type::Trit } else { Type::Word }))
                    }
                    BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                        let (ta, tb) = (self.expr(a, None)?, self.expr(b, None)?);
                        if !numeric(ta) || !numeric(tb) {
                            return d(line, col, format!("ordem entre {} e {}: só números", tname(ta), tname(tb)));
                        }
                        Ok(Some(Type::Bool))
                    }
                    BinOp::Eq | BinOp::Ne => {
                        let (ta, tb) = (self.expr(a, None)?, self.expr(b, None)?);
                        if !(numeric(ta) && numeric(tb) || ta == Some(Type::Bool) && tb == ta) {
                            return d(line, col, format!("igualdade entre {} e {}", tname(ta), tname(tb)));
                        }
                        Ok(Some(Type::Bool))
                    }
                    BinOp::And | BinOp::Or => {
                        let (ta, tb) = (self.expr(a, Some(Type::Bool))?, self.expr(b, Some(Type::Bool))?);
                        if ta != Some(Type::Bool) || tb != Some(Type::Bool) {
                            return d(line, col, format!("'&&'/'||' sobre {} e {}: só bool", tname(ta), tname(tb)));
                        }
                        Ok(ta)
                    }
                    BinOp::Kmin | BinOp::Kmax => {
                        // lógica de Kleene: os dois lados trit, ou os dois bool
                        let hint = if Self::trit_lit(a) || Self::trit_lit(b) { Some(Type::Trit) } else { want };
                        let (ta, tb) = (self.expr(a, hint)?, self.expr(b, hint)?);
                        match (ta, tb) {
                            (Some(Type::Trit), Some(Type::Trit)) | (Some(Type::Bool), Some(Type::Bool)) => Ok(ta),
                            _ => d(
                                line,
                                col,
                                format!("'&'/'|' sobre {} e {}: só trit com trit ou bool com bool", tname(ta), tname(tb)),
                            ),
                        }
                    }
                }
            }
        }
    }

    fn call(&mut self, name: &str, args: &[Expr], line: usize, col: usize) -> R<V> {
        if name == "exit" {
            return d(line, col, "exit não retorna: use como comando, não como valor");
        }
        let (ptys, ret): (Vec<Type>, V) = match builtin(name) {
            Some((n, mode)) => {
                if mode.is_some_and(|m| m != self.mode) {
                    let onde = if self.mode == Mode::User { "em programa de usuário" } else { "no núcleo" };
                    return d(line, col, format!("'{name}' não existe {onde} (docs/16)"));
                }
                (vec![Type::Word; n], builtin_ret(name))
            }
            None => match self.sigs.get(name) {
                Some((p, r)) => (p.to_vec(), *r),
                None => return d(line, col, format!("função '{name}' não existe")),
            },
        };
        if args.len() != ptys.len() {
            return d(line, col, format!("'{name}' espera {} argumento(s), recebeu {}", ptys.len(), args.len()));
        }
        if ptys.len() > 9 {
            return d(line, col, format!("'{name}': no máximo 9 argumentos (a1..a9)"));
        }
        if name.starts_with("csr_") {
            // o CSR é parte da instrução, não um valor: literal obrigatório
            let Expr::Num(c) = &args[0] else {
                return d(line, col, format!("'{name}': o CSR tem de ser um número literal (1..12)"));
            };
            let ok = if name == "csr_read" { (1..=12).contains(c) } else { csr_writable(*c as u32) && *c > 0 };
            if !ok {
                return d(line, col, format!("'{name}': CSR {c} não existe ou não aceita escrita"));
            }
        }
        for (i, (a, t)) in args.iter().zip(&ptys).enumerate() {
            self.expect(a, *t, &format!("argumento {} de '{name}'", i + 1))?;
        }
        Ok(ret)
    }
}

pub fn check(p: &Program, mode: Mode) -> R<()> {
    let mut sigs = HashMap::new();
    for f in &p.fns {
        if builtin(&f.name).is_some() {
            return d(f.line, f.col, format!("'{}' é embutida e não pode ser redefinida", f.name));
        }
        if sigs.insert(f.name.as_str(), (&f.ptys[..], f.ret)).is_some() {
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
            Some((p, _)) if !p.is_empty() => return d(1, 1, "main não pode ter parâmetros"),
            Some((_, r)) if *r != Some(Type::Word) => return d(1, 1, "main devolve o código de saída: 'fn main() -> word'"),
            Some(_) => {}
        }
    }
    let mut ck = Ck { mode, sigs, statics, scopes: Vec::new(), ret: None };
    for f in &p.fns {
        ck.fn_body(f)?;
    }
    Ok(())
}

impl<'a> Ck<'a> {
    fn fn_body(&mut self, f: &Fn) -> R<()> {
        self.scopes.push(HashMap::new());
        self.ret = f.ret;
        for (p, t) in f.params.iter().zip(&f.ptys) {
            // parâmetros são imutáveis, como em Rust sem `mut`
            self.declare(p, false, *t, f.line, f.col)?;
        }
        for s in &f.body.stmts {
            self.stmt(s)?;
        }
        self.scopes.pop();
        Ok(())
    }
}
