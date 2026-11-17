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
//! `putc`, `putdec`, `exit`, `yield`, `time`, `recv`, `shift`. No modo
//! núcleo: `static mut` (dados do `.data` do núcleo, com `a[i]` verificado)
//! e as intrínsecas de sistema `csr_*`, `phys_*`, `wfi`, `tlbi` (docs/16).
//!
//! O que ainda não existe: structs, enum, `Option`/`Result`, traits,
//! borrowing/lifetimes, arrays locais e strings. Ver docs/11 e docs/15.

pub mod check;
pub mod gen;
pub mod ir;
pub mod irgen;
pub mod lex;
pub mod lower;
pub mod parse;

#[derive(Debug)]
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

pub use ir::Mode;

/// Compila um programa de usuário `.t27r` para assembly T27 (pronto para
/// `t27 asm`, com os `.include` resolvidos em `src/os/user`). Caminho
/// único: fonte → TIR → `.tas` (docs/13, docs/14).
pub fn compile(src: &str) -> Result<String, Diag> {
    compile_mode(src, Mode::User)
}

/// Fonte do núcleo (`static mut`, `csr_*`, `phys_*`) → biblioteca `.tas`
/// para `.include` no núcleo (docs/14 §6).
pub fn compile_kernel(src: &str) -> Result<String, Diag> {
    compile_mode(src, Mode::Priv)
}

pub fn compile_mode(src: &str, mode: Mode) -> Result<String, Diag> {
    irgen::generate(&to_ir_mode(src, mode)?, 1).map_err(|msg| Diag { line: 0, col: 0, msg })
}

/// O gerador de pilha da Fase 7, direto da AST. Não é mais caminho de
/// compilação: fica como ORÁCULO dos testes diferenciais (uma segunda
/// implementação independente da mesma linguagem). Só programas de usuário
/// sem `static`.
pub fn compile_pilha(src: &str) -> Result<String, Diag> {
    let prog = front(src, Mode::User)?;
    if !prog.statics.is_empty() {
        return Err(Diag { line: 0, col: 0, msg: "o gerador de pilha não conhece static".into() });
    }
    Ok(gen::generate(&prog))
}

/// Fonte → programa aceito pela checagem no modo dado.
pub fn front(src: &str, mode: Mode) -> Result<parse::Program, Diag> {
    let toks = lex::lex(src).map_err(|e| Diag { line: e.line, col: e.col, msg: e.msg })?;
    let mut p = parse::Parser::new(toks);
    let prog = p
        .program()
        .map_err(|e| Diag { line: e.line, col: e.col, msg: e.msg })?;
    check::check(&prog, mode).map_err(|e| Diag { line: e.line, col: e.col, msg: e.msg })?;
    Ok(prog)
}

/// Fonte de usuário → TIR validado (docs/13).
pub fn to_ir(src: &str) -> Result<ir::Module, Diag> {
    to_ir_mode(src, Mode::User)
}

pub fn to_ir_mode(src: &str, mode: Mode) -> Result<ir::Module, Diag> {
    let m = lower::lower(&front(src, mode)?, mode).map_err(|msg| Diag { line: 0, col: 0, msg })?;
    // o lower só produz TIR válido; se não, é defeito do compilador
    ir::validate(&m, 1).map_err(|e| Diag { line: 0, col: 0, msg: format!("TIR inválido (defeito do t27r): {e}") })?;
    Ok(m)
}
