//! TENERURA T27 — conjunto de instruções: codificação, decodificação e
//! desmontagem (docs/07-isa-t27.md §3–§4).
//!
//! Toda instrução ocupa UMA palavra (27 trits), campos em posições fixas:
//!
//! ```text
//!   trits   26..22   21..19   18..16   15..13   12..0
//!   R       op       a        b        c        (zero)
//!   I       op       a        b        imm16 (15..0)
//!   J       op       a        imm19 (18..0)
//!   N       op       (zero)
//! ```
//!
//! Campos são números balanceados: registrador ∈ [−13, 13], opcode ∈
//! [−121, 121], imediatos com sinal SEM extensão de sinal (extensão de
//! largura por zero preserva o valor — Fase 4A). Decodificação ESTRITA:
//! campo não usado ≠ 0 ⇒ instrução ilegal; cada instrução válida tem uma
//! única codificação. Opcode 0 é ilegal para sempre (memória zerada não
//! executa). Opcode negativo ⇔ instrução privilegiada.

use crate::word::{half, Word, POW3};

/// Número de registradores (campo de 3 trits: −13..=13).
pub const NREGS: usize = 27;
/// Limites dos imediatos.
pub const IMM13_MAX: i64 = half(13);
pub const IMM16_MAX: i64 = half(16);
pub const IMM19_MAX: i64 = half(19);
/// LUI carrega os 11 trits altos: |imm| ≤ (3¹¹−1)/2.
pub const LUI_MAX: i64 = half(11);
/// Posição a partir da qual LUI escreve (trit 16).
pub const LUI_SHIFT: usize = 16;

/// Formato de codificação.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Fmt {
    /// três registradores
    R,
    /// dois registradores + imm16
    I,
    /// um registrador + imm19
    J,
    /// sem operandos
    N,
}

macro_rules! ops {
    ($( $name:ident = $code:expr, $mn:expr, $fmt:ident; )*) => {
        /// Operação (opcode balanceado de 5 trits).
        #[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
        pub enum Op { $( $name ),* }

        impl Op {
            pub const ALL: &'static [Op] = &[ $( Op::$name ),* ];

            pub const fn code(self) -> i64 {
                match self { $( Op::$name => $code ),* }
            }
            pub fn from_code(c: i64) -> Option<Op> {
                match c { $( $code => Some(Op::$name), )* _ => None }
            }
            pub const fn mnemonic(self) -> &'static str {
                match self { $( Op::$name => $mn ),* }
            }
            pub fn from_mnemonic(s: &str) -> Option<Op> {
                match s { $( $mn => Some(Op::$name), )* _ => None }
            }
            pub const fn fmt(self) -> Fmt {
                match self { $( Op::$name => Fmt::$fmt ),* }
            }
        }
    };
}

ops! {
    // ── aritmética de palavra (contratos da ALU auditada) ──
    Add  = 1,  "add",  R;   // a ← wrap(b + c)
    Sub  = 2,  "sub",  R;   // a ← wrap(b − c)
    Addc = 3,  "addc", R;   // a ← carry trit de b + c
    Mul  = 4,  "mul",  R;   // a ← lo(b · c)
    Mulh = 5,  "mulh", R;   // a ← hi(b · c)
    Div  = 6,  "div",  R;   // a ← quociente centrado
    Rem  = 7,  "rem",  R;   // a ← resto centrado
    Cmp  = 8,  "cmp",  R;   // a ← sinal(b − c) como trit
    Sh   = 9,  "sh",   R;   // a ← b deslocado por c (sinal = sentido)
    Kmin = 10, "kmin", R;   // a ← min trit a trit (∧ de Kleene)
    Kmax = 11, "kmax", R;   // a ← max trit a trit (∨ de Kleene)
    // ── imediatos ──
    Addi = 12, "addi", I;   // a ← wrap(b + imm)
    Shi  = 13, "shi",  I;   // a ← b deslocado por imm
    Lui  = 14, "lui",  I;   // a ← imm · 3¹⁶ (b = zr; |imm| ≤ LUI_MAX)
    // ── memória (palavra) ──
    Ld   = 15, "ld",   I;   // a ← mem[b + imm]
    St   = 16, "st",   I;   // mem[b + imm] ← a
    // ── controle: desvia se cmp(a, b) ∈ classe; alvo = PC + imm ──
    Beq  = 17, "beq",  I;
    Bne  = 18, "bne",  I;
    Blt  = 19, "blt",  I;
    Bge  = 20, "bge",  I;
    Bgt  = 21, "bgt",  I;
    Ble  = 22, "ble",  I;
    Jal  = 23, "jal",  J;   // a ← PC+1; PC ← PC + imm
    Jalr = 24, "jalr", I;   // t ← b + imm; a ← PC+1; PC ← t
    // ── entradas voluntárias no núcleo ──
    Sys  = 25, "sys",  N;   // trap causa 0
    Brk  = 26, "brk",  N;   // trap causa −6
    // ── privilegiadas (opcode negativo) ──
    Eret = -1, "eret", N;   // retorno de trap
    Wfi  = -2, "wfi",  N;   // espera interrupção pendente
    Csrr = -3, "csrr", I;   // a ← csr[imm]        (b = zr)
    Csrw = -4, "csrw", I;   // csr[imm] ← b        (a = zr)
    Csrx = -5, "csrx", I;   // t ← csr[imm]; csr[imm] ← b; a ← t
    Tlbi = -6, "tlbi", N;   // invalida traduções em cache
}

impl Op {
    #[inline]
    pub const fn privileged(self) -> bool {
        self.code() < 0
    }
}

/// Instrução decodificada. `a`, `b`, `c` são números de registrador
/// (−13..=13) nas posições de campo; o papel de cada um depende da operação.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Instr {
    pub op: Op,
    pub a: i8,
    pub b: i8,
    pub c: i8,
    pub imm: i64,
}

impl Instr {
    pub const fn r(op: Op, a: i8, b: i8, c: i8) -> Instr {
        Instr { op, a, b, c, imm: 0 }
    }
    pub const fn i(op: Op, a: i8, b: i8, imm: i64) -> Instr {
        Instr { op, a, b, c: 0, imm }
    }
    pub const fn j(op: Op, a: i8, imm: i64) -> Instr {
        Instr { op, a, b: 0, c: 0, imm }
    }
    pub const fn n(op: Op) -> Instr {
        Instr { op, a: 0, b: 0, c: 0, imm: 0 }
    }
}

/// Por que uma codificação/decodificação foi recusada.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum IsaError {
    /// opcode sem instrução atribuída (inclui o opcode 0)
    UnknownOpcode,
    /// campo que deveria ser zero não é (decodificação estrita)
    NonCanonical,
    /// registrador fora de −13..=13
    BadRegister,
    /// imediato fora do alcance do campo
    ImmRange,
}

#[inline]
fn reg_ok(r: i8) -> bool {
    (-13..=13).contains(&r)
}

/// Verifica as restrições de forma de uma instrução (as mesmas para
/// codificar e para aceitar uma decodificação).
fn validate(ins: &Instr) -> Result<(), IsaError> {
    if !(reg_ok(ins.a) && reg_ok(ins.b) && reg_ok(ins.c)) {
        return Err(IsaError::BadRegister);
    }
    match ins.op.fmt() {
        Fmt::R => {
            if ins.imm != 0 {
                return Err(IsaError::NonCanonical);
            }
        }
        Fmt::I => {
            if ins.c != 0 {
                return Err(IsaError::NonCanonical);
            }
            if ins.imm.abs() > IMM16_MAX {
                return Err(IsaError::ImmRange);
            }
        }
        Fmt::J => {
            if ins.b != 0 || ins.c != 0 {
                return Err(IsaError::NonCanonical);
            }
            if ins.imm.abs() > IMM19_MAX {
                return Err(IsaError::ImmRange);
            }
        }
        Fmt::N => {
            if ins.a != 0 || ins.b != 0 || ins.c != 0 || ins.imm != 0 {
                return Err(IsaError::NonCanonical);
            }
        }
    }
    // restrições específicas
    match ins.op {
        Op::Lui => {
            if ins.b != 0 {
                return Err(IsaError::NonCanonical);
            }
            if ins.imm.abs() > LUI_MAX {
                return Err(IsaError::ImmRange);
            }
        }
        Op::Csrr => {
            if ins.b != 0 {
                return Err(IsaError::NonCanonical);
            }
        }
        Op::Csrw => {
            if ins.a != 0 {
                return Err(IsaError::NonCanonical);
            }
        }
        _ => {}
    }
    Ok(())
}

/// Codifica uma instrução numa palavra. A soma de campos balanceados em
/// posições disjuntas É a composição dos dígitos — sem ajuste de empréstimo.
pub fn encode(ins: &Instr) -> Result<Word, IsaError> {
    validate(ins)?;
    let mut v = ins.op.code() * POW3[22] + ins.a as i64 * POW3[19];
    match ins.op.fmt() {
        Fmt::R => v += ins.b as i64 * POW3[16] + ins.c as i64 * POW3[13],
        Fmt::I => v += ins.b as i64 * POW3[16] + ins.imm,
        Fmt::J => v += ins.imm,
        Fmt::N => {}
    }
    Ok(Word::lit(v))
}

/// Decodifica uma palavra. Erro ⇒ a máquina levanta "instrução ilegal".
pub fn decode(w: Word) -> Result<Instr, IsaError> {
    let op = Op::from_code(w.field(22, 5)).ok_or(IsaError::UnknownOpcode)?;
    let a = w.field(19, 3) as i8;
    let ins = match op.fmt() {
        Fmt::R => {
            if w.field(0, 13) != 0 {
                return Err(IsaError::NonCanonical);
            }
            Instr::r(op, a, w.field(16, 3) as i8, w.field(13, 3) as i8)
        }
        Fmt::I => Instr::i(op, a, w.field(16, 3) as i8, w.field(0, 16)),
        Fmt::J => Instr::j(op, a, w.field(0, 19)),
        Fmt::N => {
            if w.field(0, 22) != 0 {
                return Err(IsaError::NonCanonical);
            }
            Instr::n(op)
        }
    };
    validate(&ins)?;
    Ok(ins)
}

// ───────────────────────── registradores ─────────────────────────

/// Nome canônico do registrador (ABI — docs/07-isa-t27.md §3.2):
/// zr = 0; a1..a9 = +1..+9 (argumentos/mensagem); t1..t4 = +10..+13
/// (temporários); s1..s11 = −1..−11 (preservados); lr = −12; sp = −13.
/// O SINAL do número dá a classe: positivo = volátil, negativo = preservado.
pub fn reg_name(r: i8) -> &'static str {
    const POS: [&str; 13] = [
        "a1", "a2", "a3", "a4", "a5", "a6", "a7", "a8", "a9", "t1", "t2", "t3", "t4",
    ];
    const NEG: [&str; 13] = [
        "s1", "s2", "s3", "s4", "s5", "s6", "s7", "s8", "s9", "s10", "s11", "lr", "sp",
    ];
    match r {
        0 => "zr",
        1..=13 => POS[(r - 1) as usize],
        -13..=-1 => NEG[(-r - 1) as usize],
        _ => "r?",
    }
}

/// Número do registrador a partir do nome.
pub fn reg_from_name(s: &str) -> Option<i8> {
    (-13..=13i8).find(|&r| reg_name(r) == s)
}

pub const ZR: i8 = 0;
pub const A1: i8 = 1;
pub const A2: i8 = 2;
pub const A3: i8 = 3;
pub const T1: i8 = 10;
pub const T4: i8 = 13;
pub const LR: i8 = -12;
pub const SP: i8 = -13;

// ───────────────────────── CSRs ─────────────────────────

/// Registradores de controle (acesso só em modo núcleo).
pub mod csr {
    pub const STATUS: i64 = 1;
    pub const TVEC: i64 = 2;
    pub const EPC: i64 = 3;
    pub const CAUSE: i64 = 4;
    pub const TVAL: i64 = 5;
    pub const SCRATCH: i64 = 6;
    pub const PTROOT: i64 = 7;
    pub const IMASK: i64 = 8;
    /// só leitura: linhas pendentes
    pub const IPEND: i64 = 9;
    /// só leitura: menor linha pendente ∧ habilitada (0 = nenhuma)
    pub const IRQN: i64 = 10;
    /// só leitura: instruções executadas desde o reset
    pub const TIME: i64 = 11;
    pub const TIMECMP: i64 = 12;

    pub fn name(n: i64) -> Option<&'static str> {
        Some(match n {
            STATUS => "status",
            TVEC => "tvec",
            EPC => "epc",
            CAUSE => "cause",
            TVAL => "tval",
            SCRATCH => "scratch",
            PTROOT => "ptroot",
            IMASK => "imask",
            IPEND => "ipend",
            IRQN => "irqn",
            TIME => "time",
            TIMECMP => "timecmp",
            _ => return None,
        })
    }

    pub fn from_name(s: &str) -> Option<i64> {
        (1..=12).find(|&n| name(n) == Some(s))
    }
}

/// Causas de trap. O SINAL classifica: negativo = falha (síncrona),
/// zero = chamada de sistema, positivo = interrupção (número da linha).
pub mod cause {
    pub const SYSCALL: i64 = 0;
    pub const ILLEGAL: i64 = -1;
    pub const PRIVILEGE: i64 = -2;
    pub const FETCH_FAULT: i64 = -3;
    pub const LOAD_FAULT: i64 = -4;
    pub const STORE_FAULT: i64 = -5;
    pub const BREAKPOINT: i64 = -6;
    pub const BUS_ERROR: i64 = -7;
    pub const IRQ_TIMER: i64 = 1;
    pub const IRQ_CONSOLE: i64 = 2;

    pub fn name(c: i64) -> &'static str {
        match c {
            SYSCALL => "syscall",
            ILLEGAL => "instrução ilegal",
            PRIVILEGE => "instrução privilegiada",
            FETCH_FAULT => "falha de busca",
            LOAD_FAULT => "falha de leitura",
            STORE_FAULT => "falha de escrita",
            BREAKPOINT => "breakpoint",
            BUS_ERROR => "erro de barramento",
            IRQ_TIMER => "irq timer",
            IRQ_CONSOLE => "irq console",
            c if c > 0 => "irq",
            _ => "causa desconhecida",
        }
    }
}

// ───────────────────────── desmontagem ─────────────────────────

/// Desmonta uma instrução na sintaxe aceita pelo montador. `pc` permite
/// mostrar o alvo absoluto de desvios (`None` ⇒ deslocamento relativo).
pub fn disasm(ins: &Instr, pc: Option<i64>) -> String {
    let (a, b, c) = (reg_name(ins.a), reg_name(ins.b), reg_name(ins.c));
    let m = ins.op.mnemonic();
    let target = |off: i64| match pc {
        Some(p) => format!("{}", p + off),
        None => format!(".{:+}", off),
    };
    match ins.op {
        Op::Add | Op::Sub | Op::Addc | Op::Mul | Op::Mulh | Op::Div | Op::Rem | Op::Cmp
        | Op::Sh | Op::Kmin | Op::Kmax => format!("{m} {a}, {b}, {c}"),
        Op::Addi | Op::Shi => format!("{m} {a}, {b}, {}", ins.imm),
        Op::Lui => format!("{m} {a}, {}", ins.imm),
        Op::Ld | Op::St => format!("{m} {a}, {}({b})", ins.imm),
        Op::Beq | Op::Bne | Op::Blt | Op::Bge | Op::Bgt | Op::Ble => {
            format!("{m} {a}, {b}, {}", target(ins.imm))
        }
        Op::Jal => format!("{m} {a}, {}", target(ins.imm)),
        Op::Jalr => format!("{m} {a}, {}({b})", ins.imm),
        Op::Sys | Op::Brk | Op::Eret | Op::Wfi | Op::Tlbi => m.to_string(),
        Op::Csrr => format!("{m} {a}, {}", csr_disp(ins.imm)),
        Op::Csrw => format!("{m} {}, {b}", csr_disp(ins.imm)),
        Op::Csrx => format!("{m} {a}, {}, {b}", csr_disp(ins.imm)),
    }
}

fn csr_disp(n: i64) -> String {
    match csr::name(n) {
        Some(s) => s.to_string(),
        None => n.to_string(),
    }
}

/// Desmonta uma palavra; palavra que não decodifica vira `.word`.
pub fn disasm_word(w: Word, pc: Option<i64>) -> String {
    match decode(w) {
        Ok(ins) => disasm(&ins, pc),
        Err(_) => format!(".word {}", w.val()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opcode_table_is_consistent() {
        let mut seen = std::collections::HashSet::new();
        for &op in Op::ALL {
            assert!(op.code() != 0, "opcode 0 é reservado como ilegal");
            assert!(op.code().abs() <= half(5));
            assert!(seen.insert(op.code()), "opcode duplicado {:?}", op);
            assert_eq!(Op::from_code(op.code()), Some(op));
            assert_eq!(Op::from_mnemonic(op.mnemonic()), Some(op));
        }
        assert_eq!(Op::from_code(0), None);
    }

    #[test]
    fn zero_word_is_illegal() {
        assert_eq!(decode(Word::ZERO), Err(IsaError::UnknownOpcode));
    }

    #[test]
    fn register_names_roundtrip() {
        for r in -13..=13i8 {
            assert_eq!(reg_from_name(reg_name(r)), Some(r));
        }
        assert_eq!(reg_name(0), "zr");
        assert_eq!(reg_name(-13), "sp");
        assert_eq!(reg_name(-12), "lr");
    }

    #[test]
    fn encode_decode_boundaries() {
        let cases = [
            Instr::r(Op::Add, 13, -13, 13),
            Instr::r(Op::Kmax, -13, 13, -13),
            Instr::i(Op::Addi, 1, 0, IMM16_MAX),
            Instr::i(Op::Addi, 1, 0, -IMM16_MAX),
            Instr::i(Op::Lui, 5, 0, LUI_MAX),
            Instr::i(Op::Lui, 5, 0, -LUI_MAX),
            Instr::j(Op::Jal, -12, IMM19_MAX),
            Instr::j(Op::Jal, 0, -IMM19_MAX),
            Instr::n(Op::Sys),
            Instr::n(Op::Eret),
            Instr::i(Op::Csrx, 13, 13, csr::SCRATCH),
        ];
        for ins in cases {
            let w = encode(&ins).unwrap();
            assert_eq!(decode(w), Ok(ins), "{:?}", ins);
        }
        assert_eq!(encode(&Instr::i(Op::Addi, 1, 0, IMM16_MAX + 1)), Err(IsaError::ImmRange));
        assert_eq!(encode(&Instr::i(Op::Lui, 1, 0, LUI_MAX + 1)), Err(IsaError::ImmRange));
        assert_eq!(encode(&Instr::i(Op::Lui, 1, 2, 5)), Err(IsaError::NonCanonical));
    }
}
