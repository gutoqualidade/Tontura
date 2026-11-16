//! TENERURA T27 — a máquina: CPU, memória, tradução de endereços, traps e
//! dispositivos (docs/07-isa-t27.md §5–§8).
//!
//! Emulador no nível de INSTRUÇÃO (não de portas): cada `step` executa uma
//! instrução ou toma um trap. É totalmente determinístico — o "tempo" da
//! máquina é o contador de passos, não o relógio do hospedeiro.
//!
//! O que a máquina garante por construção (e os testes de isolamento
//! verificam): em modo usuário, (1) instrução privilegiada ⇒ trap, (2) todo
//! acesso à memória passa pela tabela de páginas apontada por PTROOT,
//! (3) a única forma de voltar ao modo núcleo é um trap para TVEC.

use std::collections::VecDeque;

use crate::isa::{cause, csr, decode, Op};
use crate::word::{half, Word, POW3};

/// Página: 3⁶ = 729 palavras, CENTRADA no múltiplo de 729.
pub const PAGE_TRITS: usize = 6;
pub const PAGE_SIZE: i64 = POW3[PAGE_TRITS];
pub const PAGE_HALF: i64 = half(PAGE_TRITS);
/// Endereço virtual canônico: 18 trits (9 trits altos zero).
pub const VA_MAX: i64 = half(18);

/// Flags de PTE (6 trits baixos da entrada).
pub const PTE_LEAF: i64 = 1; // trit 0 = +1: página
pub const PTE_TABLE: i64 = -1; // trit 0 = −1: ponteiro para tabela de 2º nível
pub const PTE_W: i64 = 3; // trit 1 = +1: escrita permitida
pub const PTE_X: i64 = 9; // trit 2 = +1: execução permitida

/// Dispositivos mapeados em memória: um por página, fora da RAM.
pub const DEV_CONSOLE: i64 = POW3[15];
pub const DEV_POWER: i64 = POW3[15] + PAGE_SIZE;
/// Registradores do console (deslocamento a partir do centro da página).
pub const CON_TX: i64 = 0;
pub const CON_RX: i64 = 1;
pub const CON_STATUS: i64 = 2;

/// RAM padrão: 3¹² palavras = 729 quadros, endereços [−265720, +265720].
pub const DEFAULT_RAM_HALF: i64 = half(12);

/// Arredonda v/729 ao inteiro mais próximo (sem empates: 729 é ímpar).
#[inline]
pub fn page_of(addr: i64) -> i64 {
    (addr + PAGE_HALF).div_euclid(PAGE_SIZE)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Access {
    Fetch,
    Load,
    Store,
}

/// O que aconteceu no último passo.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Step {
    /// instrução executada normalmente
    Exec,
    /// trap tomado (causa)
    Trap(i64),
    /// ERET executado (saída do núcleo)
    Eret,
    /// WFI sem fonte de despertar possível: a máquina ficaria parada para sempre
    Idle,
    /// dispositivo de energia desligou a máquina (código)
    Halted(i64),
    /// falha ao buscar o próprio tratador de traps em modo núcleo
    DoubleFault,
}

/// Por que `run` parou.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Stop {
    Halted(i64),
    StepLimit,
    Idle,
    DoubleFault,
}

/// Contadores (determinísticos) para medição.
#[derive(Clone, Debug, Default)]
pub struct Stats {
    pub instr_user: u64,
    pub instr_kernel: u64,
    pub traps_fault: u64,
    pub traps_syscall: u64,
    pub traps_irq: u64,
    /// das interrupções, quantas vieram de linhas de dispositivo (≠ timer)
    pub traps_irq_dev: u64,
    pub tlbi: u64,
}

pub struct Machine {
    /// registradores: índice = número + 13; o índice 13 (zr) é sempre zero
    pub regs: [Word; 27],
    pub pc: Word,
    // STATUS decomposto
    pub kernel: bool,
    pub ie: bool,
    pub pmode_kernel: bool,
    pub pie: bool,
    pub tvec: Word,
    pub epc: Word,
    pub cause: Word,
    pub tval: Word,
    pub scratch: Word,
    pub ptroot: Word,
    /// máscara de linhas habilitadas (bit i ↔ linha i+1)
    pub imask: u32,
    /// linhas levantadas externamente (testes/dispositivos futuros)
    pub irq_ext: u32,
    pub timecmp: Word,
    /// passos desde o reset
    pub time: i64,
    ram: Vec<Word>,
    ram_half: i64,
    pub console_out: Vec<i64>,
    pub console_in: VecDeque<i64>,
    pub halted: Option<i64>,
    pub stats: Stats,
    /// Sonda de teste: acessos de dados feitos EM MODO NÚCLEO a endereços
    /// de RAM ≥ `watch_from` são contados em `watch_hits`.
    pub watch_from: Option<i64>,
    pub watch_hits: u64,
}

type Fault = (i64, Word);

impl Machine {
    /// Máquina em estado de reset: modo núcleo, interrupções desligadas,
    /// PC = 0, CSRs zerados, a1 = meia-extensão da RAM (protocolo de boot).
    pub fn new(ram_half: i64) -> Machine {
        assert!(ram_half > 0 && ram_half < DEV_CONSOLE - PAGE_HALF);
        let mut m = Machine {
            regs: [Word::ZERO; 27],
            pc: Word::ZERO,
            kernel: true,
            ie: false,
            pmode_kernel: false,
            pie: false,
            tvec: Word::ZERO,
            epc: Word::ZERO,
            cause: Word::ZERO,
            tval: Word::ZERO,
            scratch: Word::ZERO,
            ptroot: Word::ZERO,
            imask: 0,
            irq_ext: 0,
            timecmp: Word::ZERO,
            time: 0,
            ram: vec![Word::ZERO; (2 * ram_half + 1) as usize],
            ram_half,
            console_out: Vec::new(),
            console_in: VecDeque::new(),
            halted: None,
            stats: Stats::default(),
            watch_from: None,
            watch_hits: 0,
        };
        m.set_reg(1, Word::lit(ram_half));
        m
    }

    pub fn ram_half(&self) -> i64 {
        self.ram_half
    }

    /// Preenche RAM e registradores com palavras pseudoaleatórias válidas
    /// (teste de robustez: o núcleo não pode depender de memória zerada).
    /// Chamar ANTES de carregar a imagem. PRNG xorshift64*, semente registrada.
    pub fn randomize(&mut self, seed: u64) {
        let mut x = seed | 1;
        let mut next = move || {
            x ^= x >> 12;
            x ^= x << 25;
            x ^= x >> 27;
            x.wrapping_mul(0x2545_F491_4F6C_DD1D)
        };
        for w in self.ram.iter_mut() {
            *w = Word::wrap128(next() as i128);
        }
        for r in 0..27 {
            if r != 13 {
                self.regs[r] = Word::wrap128(next() as i128);
            }
        }
        self.set_reg(1, Word::lit(self.ram_half));
    }

    #[inline]
    pub fn reg(&self, r: i8) -> Word {
        self.regs[(r + 13) as usize]
    }

    #[inline]
    pub fn set_reg(&mut self, r: i8, v: Word) {
        if r != 0 {
            self.regs[(r + 13) as usize] = v;
        }
    }

    // ───────────── memória física (nível do arnês: só RAM) ─────────────

    #[inline]
    fn ram_index(&self, pa: i64) -> Option<usize> {
        if pa >= -self.ram_half && pa <= self.ram_half {
            Some((pa + self.ram_half) as usize)
        } else {
            None
        }
    }

    /// Lê uma palavra da RAM (sem efeitos colaterais; `None` fora da RAM).
    pub fn peek(&self, pa: i64) -> Option<Word> {
        self.ram_index(pa).map(|i| self.ram[i])
    }

    /// Escreve uma palavra na RAM (carga de imagem / injeção de falhas).
    pub fn poke(&mut self, pa: i64, w: Word) -> bool {
        match self.ram_index(pa) {
            Some(i) => {
                self.ram[i] = w;
                true
            }
            None => false,
        }
    }

    /// Carrega um bloco contíguo na RAM a partir de `base`.
    pub fn load(&mut self, base: i64, words: &[Word]) {
        for (i, &w) in words.iter().enumerate() {
            assert!(self.poke(base + i as i64, w), "imagem fora da RAM em {}", base + i as i64);
        }
    }

    // ───────────── barramento físico (com dispositivos) ─────────────

    fn phys_read(&mut self, pa: i64) -> Result<Word, ()> {
        if let Some(i) = self.ram_index(pa) {
            return Ok(self.ram[i]);
        }
        match pa - DEV_CONSOLE {
            CON_TX => Ok(Word::ZERO),
            CON_RX => Ok(Word::lit(self.console_in.pop_front().unwrap_or(-1))),
            CON_STATUS => Ok(Word::lit(self.console_in.len() as i64)),
            _ => {
                if pa == DEV_POWER {
                    Ok(Word::ZERO)
                } else {
                    Err(())
                }
            }
        }
    }

    fn phys_write(&mut self, pa: i64, w: Word) -> Result<(), ()> {
        if let Some(i) = self.ram_index(pa) {
            self.ram[i] = w;
            return Ok(());
        }
        if pa == DEV_CONSOLE + CON_TX {
            self.console_out.push(w.val());
            Ok(())
        } else if pa == DEV_POWER {
            self.halted = Some(w.val());
            Ok(())
        } else {
            Err(())
        }
    }

    // ───────────── tradução de endereços ─────────────

    /// Traduz um endereço. Em modo núcleo: identidade (endereçamento
    /// físico). Em modo usuário: tabela de 2 níveis, índices balanceados
    /// de 6 trits, páginas e tabelas centradas.
    pub fn translate(&self, va: i64, acc: Access) -> Result<i64, Fault> {
        if self.kernel {
            return Ok(va);
        }
        let fault = (
            match acc {
                Access::Fetch => cause::FETCH_FAULT,
                Access::Load => cause::LOAD_FAULT,
                Access::Store => cause::STORE_FAULT,
            },
            Word::wrap(va),
        );
        if va.abs() > VA_MAX {
            return Err(fault);
        }
        let vpn = page_of(va);
        let off = va - vpn * PAGE_SIZE;
        let i1 = page_of(vpn);
        let i2 = vpn - i1 * PAGE_SIZE;
        let pte1 = self.peek(self.ptroot.val() + i1).ok_or(fault)?;
        let f1 = pte1.field(0, PAGE_TRITS);
        if f1 != PTE_TABLE {
            return Err(fault);
        }
        let pte2 = self.peek(pte1.val() - f1 + i2).ok_or(fault)?;
        let f2 = pte2.field(0, PAGE_TRITS);
        let (v, w, x) = (pte2.field(0, 1), pte2.field(1, 1), pte2.field(2, 1));
        if v != 1 || w < 0 || x < 0 {
            return Err(fault);
        }
        let ok = match acc {
            Access::Fetch => x == 1,
            Access::Load => true,
            Access::Store => w == 1,
        };
        if !ok {
            return Err(fault);
        }
        Ok(pte2.val() - f2 + off)
    }

    // ───────────── interrupções ─────────────

    /// Linhas pendentes (bit i ↔ linha i+1).
    pub fn ipend(&self) -> u32 {
        let mut p = self.irq_ext;
        if self.timer_delta() >= 0 {
            p |= 1 << (cause::IRQ_TIMER - 1);
        }
        if !self.console_in.is_empty() {
            p |= 1 << (cause::IRQ_CONSOLE - 1);
        }
        p
    }

    /// (TIME − TIMECMP) mod 3²⁷ lido como balanceado. O timer está pendente
    /// quando isto é ≥ 0. TIME é um contador MODULAR: a comparação por
    /// diferença continua correta quando o contador dá a volta (desde que
    /// os prazos fiquem a menos de meia janela de distância).
    pub fn timer_delta(&self) -> i64 {
        Word::wrap128(self.time as i128 - self.timecmp.val() as i128).val()
    }

    /// Menor linha pendente ∧ habilitada; 0 se nenhuma.
    pub fn irqn(&self) -> i64 {
        let p = self.ipend() & self.imask;
        if p == 0 {
            0
        } else {
            p.trailing_zeros() as i64 + 1
        }
    }

    fn take_trap(&mut self, c: i64, tval: Word, epc: Word) {
        self.pmode_kernel = self.kernel;
        self.pie = self.ie;
        self.kernel = true;
        self.ie = false;
        self.epc = epc;
        self.cause = Word::lit(c);
        self.tval = tval;
        self.pc = self.tvec;
        if c < 0 {
            self.stats.traps_fault += 1;
        } else if c == 0 {
            self.stats.traps_syscall += 1;
        } else {
            self.stats.traps_irq += 1;
            if c != cause::IRQ_TIMER {
                self.stats.traps_irq_dev += 1;
            }
        }
    }

    /// Trap síncrono (falha ou entrada voluntária) na instrução em PC.
    fn fault(&mut self, c: i64, tval: Word) -> Step {
        // Trap síncrono na PRIMEIRA instrução do tratador, já em modo
        // núcleo: retomar em TVEC repetiria o mesmo trap para sempre —
        // a máquina para em vez de girar.
        if self.kernel && self.pc == self.tvec {
            return Step::DoubleFault;
        }
        let epc = self.pc;
        self.take_trap(c, tval, epc);
        Step::Trap(c)
    }

    // ───────────── CSRs ─────────────

    fn status_word(&self) -> Word {
        Word::lit(
            self.kernel as i64 + 3 * self.ie as i64 + 9 * self.pmode_kernel as i64 + 27 * self.pie as i64,
        )
    }

    fn mask_to_word(mask: u32) -> Word {
        let mut v = 0i64;
        for i in 0..27 {
            if mask >> i & 1 == 1 {
                v += POW3[i];
            }
        }
        Word::lit(v)
    }

    /// Palavra → máscara de linhas; `None` se algum trit ∉ {0, +1}.
    fn word_to_mask(w: Word) -> Option<u32> {
        let mut m = 0u32;
        for (i, &d) in w.trits().iter().enumerate() {
            match d {
                0 => {}
                1 => m |= 1 << i,
                _ => return None,
            }
        }
        Some(m)
    }

    fn csr_read(&self, n: i64) -> Option<Word> {
        Some(match n {
            csr::STATUS => self.status_word(),
            csr::TVEC => self.tvec,
            csr::EPC => self.epc,
            csr::CAUSE => self.cause,
            csr::TVAL => self.tval,
            csr::SCRATCH => self.scratch,
            csr::PTROOT => self.ptroot,
            csr::IMASK => Self::mask_to_word(self.imask),
            csr::IPEND => Self::mask_to_word(self.ipend()),
            csr::IRQN => Word::lit(self.irqn()),
            csr::TIME => Word::wrap128(self.time as i128),
            csr::TIMECMP => self.timecmp,
            _ => return None,
        })
    }

    /// Escreve um CSR; `false` ⇒ instrução ilegal (CSR inexistente, só de
    /// leitura, ou valor com codificação reservada).
    fn csr_write(&mut self, n: i64, w: Word) -> bool {
        match n {
            csr::STATUS => {
                let v = w.val();
                if !(0..81).contains(&v) {
                    return false;
                }
                let d = [v % 3, v / 3 % 3, v / 9 % 3, v / 27 % 3];
                if d.iter().any(|&x| x > 1) {
                    return false;
                }
                self.kernel = d[0] == 1;
                self.ie = d[1] == 1;
                self.pmode_kernel = d[2] == 1;
                self.pie = d[3] == 1;
            }
            csr::TVEC => self.tvec = w,
            csr::EPC => self.epc = w,
            csr::CAUSE => self.cause = w,
            csr::TVAL => self.tval = w,
            csr::SCRATCH => self.scratch = w,
            csr::PTROOT => self.ptroot = w,
            csr::IMASK => match Self::word_to_mask(w) {
                Some(m) => self.imask = m,
                None => return false,
            },
            csr::TIMECMP => self.timecmp = w,
            _ => return false,
        }
        true
    }

    // ───────────── execução ─────────────

    /// Executa um passo: toma uma interrupção pendente ou executa a
    /// instrução em PC. Exceções são PRECISAS: instrução que falha não
    /// deixa efeito arquitetural além do trap.
    pub fn step(&mut self) -> Step {
        if let Some(code) = self.halted {
            return Step::Halted(code);
        }
        self.time += 1;
        if self.ie {
            let n = self.irqn();
            if n != 0 {
                let epc = self.pc;
                self.take_trap(n, Word::ZERO, epc);
                return Step::Trap(n);
            }
        }
        let pc = self.pc.val();
        let pa = match self.translate(pc, Access::Fetch) {
            Ok(pa) => pa,
            Err((c, tv)) => return self.fault(c, tv),
        };
        let w = match self.phys_read(pa) {
            Ok(w) => w,
            Err(()) => return self.fault(cause::BUS_ERROR, Word::wrap(pc)),
        };
        let ins = match decode(w) {
            Ok(i) => i,
            Err(_) => return self.fault(cause::ILLEGAL, w),
        };
        if ins.op.privileged() && !self.kernel {
            return self.fault(cause::PRIVILEGE, w);
        }
        let (ra, rb, rc) = (self.reg(ins.a), self.reg(ins.b), self.reg(ins.c));
        let next = Word::wrap(pc + 1);
        let mut new_pc = next;
        let mut result = Step::Exec;
        match ins.op {
            Op::Add => self.set_reg(ins.a, rb.add(rc).0),
            Op::Sub => self.set_reg(ins.a, rb.sub(rc).0),
            Op::Addc => self.set_reg(ins.a, Word::lit(rb.add(rc).1 as i64)),
            Op::Mul => self.set_reg(ins.a, rb.mul(rc).0),
            Op::Mulh => self.set_reg(ins.a, rb.mul(rc).1),
            Op::Div => self.set_reg(ins.a, rb.divrem(rc).0),
            Op::Rem => self.set_reg(ins.a, rb.divrem(rc).1),
            Op::Cmp => self.set_reg(ins.a, Word::lit(rb.cmp3(rc) as i64)),
            Op::Sh => self.set_reg(ins.a, rb.shift(rc.val())),
            Op::Kmin => self.set_reg(ins.a, rb.kmin(rc)),
            Op::Kmax => self.set_reg(ins.a, rb.kmax(rc)),
            Op::Addi => self.set_reg(ins.a, Word::wrap(rb.val() + ins.imm)),
            Op::Shi => self.set_reg(ins.a, rb.shift(ins.imm)),
            Op::Lui => self.set_reg(ins.a, Word::lit(ins.imm * POW3[crate::isa::LUI_SHIFT])),
            Op::Ld => {
                let va = Word::wrap(rb.val() + ins.imm).val();
                let pa = match self.translate(va, Access::Load) {
                    Ok(pa) => pa,
                    Err((c, tv)) => return self.fault(c, tv),
                };
                self.probe(pa);
                match self.phys_read(pa) {
                    Ok(v) => self.set_reg(ins.a, v),
                    Err(()) => return self.fault(cause::BUS_ERROR, Word::wrap(va)),
                }
            }
            Op::St => {
                let va = Word::wrap(rb.val() + ins.imm).val();
                let pa = match self.translate(va, Access::Store) {
                    Ok(pa) => pa,
                    Err((c, tv)) => return self.fault(c, tv),
                };
                self.probe(pa);
                if self.phys_write(pa, ra).is_err() {
                    return self.fault(cause::BUS_ERROR, Word::wrap(va));
                }
                if let Some(code) = self.halted {
                    self.count_instr();
                    self.pc = next;
                    return Step::Halted(code);
                }
            }
            Op::Beq | Op::Bne | Op::Blt | Op::Bge | Op::Bgt | Op::Ble => {
                let s = ra.cmp3(rb);
                let taken = match ins.op {
                    Op::Beq => s == 0,
                    Op::Bne => s != 0,
                    Op::Blt => s < 0,
                    Op::Bge => s >= 0,
                    Op::Bgt => s > 0,
                    _ => s <= 0,
                };
                if taken {
                    new_pc = Word::wrap(pc + ins.imm);
                }
            }
            Op::Jal => {
                self.set_reg(ins.a, next);
                new_pc = Word::wrap(pc + ins.imm);
            }
            Op::Jalr => {
                new_pc = Word::wrap(rb.val() + ins.imm);
                self.set_reg(ins.a, next);
            }
            Op::Sys => return self.fault(cause::SYSCALL, Word::ZERO),
            Op::Brk => return self.fault(cause::BREAKPOINT, Word::ZERO),
            Op::Eret => {
                self.kernel = self.pmode_kernel;
                self.ie = self.pie;
                new_pc = self.epc;
                result = Step::Eret;
            }
            Op::Wfi => {
                if self.irqn() == 0 {
                    // Avança o tempo até o timer vencer, se ele pode acordar.
                    let timer_on = self.imask & 1 != 0;
                    if timer_on {
                        self.time -= self.timer_delta(); // delta < 0: falta |delta|
                    } else {
                        // nada pode acordar a máquina (entrada externa só
                        // chega pelo arnês, entre passos)
                        self.time -= 1;
                        return Step::Idle;
                    }
                }
            }
            Op::Csrr => match self.csr_read(ins.imm) {
                Some(v) => self.set_reg(ins.a, v),
                None => return self.fault(cause::ILLEGAL, w),
            },
            Op::Csrw => {
                if !self.csr_write(ins.imm, rb) {
                    return self.fault(cause::ILLEGAL, w);
                }
            }
            Op::Csrx => {
                let old = match self.csr_read(ins.imm) {
                    Some(v) => v,
                    None => return self.fault(cause::ILLEGAL, w),
                };
                if !self.csr_write(ins.imm, rb) {
                    return self.fault(cause::ILLEGAL, w);
                }
                self.set_reg(ins.a, old);
            }
            Op::Tlbi => self.stats.tlbi += 1,
        }
        // Conta pelo modo em que a instrução EXECUTOU (ERET/CSRW STATUS
        // mudam o modo; ambas são privilegiadas ⇒ executaram em núcleo).
        if ins.op.privileged() {
            self.stats.instr_kernel += 1;
        } else {
            self.count_instr();
        }
        self.pc = new_pc;
        result
    }

    #[inline]
    fn probe(&mut self, pa: i64) {
        if let Some(from) = self.watch_from {
            if self.kernel && pa >= from && pa <= self.ram_half {
                self.watch_hits += 1;
            }
        }
    }

    #[inline]
    fn count_instr(&mut self) {
        if self.kernel {
            self.stats.instr_kernel += 1;
        } else {
            self.stats.instr_user += 1;
        }
    }

    /// Executa até parar ou até `max_steps` passos.
    pub fn run(&mut self, max_steps: u64) -> Stop {
        for _ in 0..max_steps {
            match self.step() {
                Step::Exec | Step::Trap(_) | Step::Eret => {}
                Step::Halted(c) => return Stop::Halted(c),
                Step::Idle => return Stop::Idle,
                Step::DoubleFault => return Stop::DoubleFault,
            }
        }
        Stop::StepLimit
    }

    /// Saída do console como texto (códigos = escalares Unicode; inválidos
    /// viram U+FFFD).
    pub fn console_text(&self) -> String {
        self.console_out
            .iter()
            .map(|&c| u32::try_from(c).ok().and_then(char::from_u32).unwrap_or('\u{FFFD}'))
            .collect()
    }
}
