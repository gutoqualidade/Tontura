//! TENERURA — representação e compactação de trits.
//!
//! Camadas (§3 do plano):
//! 1. **Lógica:** [`trit`] — tipos semanticamente distintos (`ArithTrit`,
//!    `KleeneTrit`, `LukasTrit`) sobre o portador [`crate::trit::S3`].
//! 2. **Trabalho:** [`pack2`] — 2 bits/trit, código `11` reservado a integridade.
//! 3. **Compacta:** [`pack3`] — base 3 densa, alcance explícito por contêiner.

pub mod alu;
pub mod pack2;
pub mod pack3;
pub mod trit;
