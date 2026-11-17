//! TENERURA T27 — máquina ternária balanceada de 27 trits.
//!
//! - [`word`]: a palavra (semântica da ALU auditada, representação `i64`).
//! - [`isa`]: instruções, codificação estrita, CSRs, causas de trap.
//! - [`machine`]: CPU + memória + tradução + traps + dispositivos.
//! - [`asm`]: montador.
//!
//! Especificação: docs/07-isa-t27.md. A arquitetura é ternária na SEMÂNTICA;
//! o hospedeiro binário é detalhe de emulação.

pub mod asm;
pub mod harness;
pub mod isa;
pub mod kcheck;
pub mod lang;
pub mod machine;
pub mod manifest;
pub mod sysimage;
pub mod word;
