//! TENERURA — linguagem de sistemas para T27: um subconjunto de Rust.
//!
//! Não é Rust compilado pelo `rustc`: o `rustc` não tem alvo para uma
//! máquina de 27 trits e escrever um backend LLVM para ela não é o caminho
//! (docs/11). É um subconjunto com a mesma sintaxe e as mesmas ideias,
//! compilado por este módulo para assembly T27 — sem dependências, sem
//! alocador, com um único tipo inteiro: a palavra de 27 trits.
//!
//! O que existe hoje: `fn`, `let`/`let mut`, `if`/`else`, `while`,
//! `return`, `match` de três vias (o nativo do ternário), funções
//! recursivas, aritmética e comparação da ALU auditada, e as embutidas
//! `putc`, `putdec`, `exit`, `yield`, `time`, `recv`.
//!
//! O que ainda não existe: structs, enum, `Option`/`Result`, traits,
//! borrowing/lifetimes, arrays e strings. Ver docs/11 §"o que falta".

pub mod check;
pub mod gen;
pub mod lex;
pub mod parse;

pub struct Diag {
    pub line: usize,
    pub col: usize,
    pub msg: String,
}

impl std::fmt::Display for Diag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.line == 0 {
            write!(f, "erro: {}", self.msg)
        } else {
            write!(f, "{}:{}: {}", self.line, self.col, self.msg)
        }
    }
}

/// Compila fonte `.t27r` para texto de assembly T27 (pronto para
/// `t27 asm`, com os `.include` resolvidos em `src/os/user`).
pub fn compile(src: &str) -> Result<String, Diag> {
    let toks = lex::lex(src).map_err(|e| Diag { line: e.line, col: e.col, msg: e.msg })?;
    let mut p = parse::Parser::new(toks);
    let prog = p
        .program()
        .map_err(|e| Diag { line: e.line, col: e.col, msg: e.msg })?;
    check::check(&prog).map_err(|e| Diag { line: e.line, col: e.col, msg: e.msg })?;
    Ok(gen::generate(&prog))
}
