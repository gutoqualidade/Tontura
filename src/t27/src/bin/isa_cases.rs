//! TENERURA — gerador de casos para a verificação cruzada da ISA
//! (Rust ↔ Haskell). Cada caso é UMA instrução executada pelo emulador a
//! partir de um estado pseudoaleatório; a saída registra o estado antes e o
//! estado depois. A implementação Haskell independente
//! (experiments/phase5/T27Ref.hs) reexecuta cada caso sobre listas de
//! dígitos e compara.
//!
//! Linha:  pc | 27 regs | maddr mval | palavra || classe | pc' | 27 regs' | mval'
//!   classe: E = executada; I = instrução ilegal; P = privilegiada, S = sys,
//!           B = brk, X = acesso fora da RAM do caso (só a classificação é comparada)
//!
//! Uso: isa_cases [n] [semente]   (padrão: 20000 casos, semente registrada)

use std::io::Write;

use tenerura_t27::isa::{cause, decode, encode, Fmt, Instr, Op, IMM16_MAX, IMM19_MAX, LUI_MAX};
use tenerura_t27::machine::{Machine, Step};
use tenerura_t27::word::{half, Word, M27, POW3};

const SEED: u64 = 0x5EED_2026_1116 ^ 0x15A0;

struct XorShift(u64);

impl XorShift {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn range(&mut self, lo: i64, hi: i64) -> i64 {
        lo + (self.next() % (hi - lo + 1) as u64) as i64
    }
    /// palavra "interessante": fronteiras, potências de 3, pequenas, ou qualquer
    fn word(&mut self) -> i64 {
        match self.next() % 8 {
            0 => self.range(-4, 4),
            1 => {
                let k = self.range(0, 26) as usize;
                let d = self.range(-1, 1);
                (if self.next() % 2 == 0 { POW3[k] } else { -POW3[k] }) + d
            }
            2 => {
                let k = self.range(1, 27) as usize;
                (if self.next() % 2 == 0 { half(k) } else { -half(k) }) + self.range(-1, 1).min(0)
            }
            3 => [M27, -M27, M27 - 1, -M27 + 1, 0][self.range(0, 4) as usize],
            4 => self.range(-40, 40),
            _ => Word::wrap128(self.next() as i128).val(),
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let n: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(20_000);
    let seed: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(SEED);
    let mut rng = XorShift(seed | 1);
    let out = std::io::stdout();
    let mut out = std::io::BufWriter::new(out.lock());
    let user_ops: Vec<Op> = Op::ALL.iter().copied().collect();

    for _ in 0..n {
        let mut m = Machine::new(2000);
        // estado inicial
        let pc = rng.range(-900, 900);
        for r in -13..=13i8 {
            m.set_reg(r, Word::wrap(rng.word()));
        }
        // célula de memória do caso: nunca a da própria instrução
        let maddr = loop {
            let a = rng.range(-1500, 1500);
            if a != pc {
                break a;
            }
        };
        let mval = Word::wrap(rng.word());

        // instrução: 1/8 palavra crua (quase sempre ilegal); senão válida
        let word = if rng.next() % 8 == 0 {
            Word::wrap(rng.word())
        } else {
            let op = user_ops[rng.range(0, user_ops.len() as i64 - 1) as usize];
            let r = |rng: &mut XorShift| rng.range(-13, 13) as i8;
            let imm = |rng: &mut XorShift, max: i64| match rng.next() % 3 {
                0 => rng.range(-30, 30),
                1 => [max, -max, max - 1, 0][rng.range(0, 3) as usize],
                _ => rng.range(-max, max),
            };
            let ins = match op.fmt() {
                Fmt::R => Instr::r(op, r(&mut rng), r(&mut rng), r(&mut rng)),
                Fmt::I => match op {
                    Op::Lui => Instr::i(op, r(&mut rng), 0, imm(&mut rng, LUI_MAX)),
                    Op::Csrr => Instr::i(op, r(&mut rng), 0, rng.range(1, 12)),
                    Op::Csrw => Instr::i(op, 0, r(&mut rng), rng.range(1, 12)),
                    Op::Csrx => Instr::i(op, r(&mut rng), r(&mut rng), rng.range(1, 12)),
                    Op::Ld | Op::St => {
                        // base + deslocamento cai exatamente na célula de memória do caso
                        let b = r(&mut rng);
                        let off = rng.range(-300, 300);
                        if b != 0 {
                            m.set_reg(b, Word::lit(maddr - off));
                            Instr::i(op, r(&mut rng), b, off)
                        } else {
                            Instr::i(op, r(&mut rng), 0, maddr.clamp(-IMM16_MAX, IMM16_MAX))
                        }
                    }
                    Op::Sh | Op::Shi => Instr::i(op, r(&mut rng), r(&mut rng), rng.range(-30, 30)),
                    _ => Instr::i(op, r(&mut rng), r(&mut rng), imm(&mut rng, IMM16_MAX)),
                },
                Fmt::J => Instr::j(op, r(&mut rng), imm(&mut rng, IMM19_MAX)),
                Fmt::N => Instr::n(op),
            };
            // deslocamento variável (SH): às vezes um contador pequeno no registrador c
            if op == Op::Sh && rng.next() % 2 == 0 && ins.c != 0 {
                m.set_reg(ins.c, Word::lit(rng.range(-30, 30)));
            }
            encode(&ins).expect("instrução gerada é válida")
        };

        // LD/ST com o registrador-base igual ao de destino: o endereço é o do
        // valor ANTES da instrução — o estado registrado abaixo já é o final
        m.poke(maddr, mval);
        m.poke(pc, word);
        m.pc = Word::lit(pc);
        m.tvec = Word::lit(1900); // tratador fictício: só para classificar o trap
        let before: Vec<i64> = (-13..=13i8).map(|r| m.reg(r).val()).collect();
        let privileged = matches!(decode(word), Ok(i) if i.op.privileged());
        let class = if privileged {
            'P'
        } else {
            match m.step() {
                Step::Exec => 'E',
                Step::Trap(c) if c == cause::ILLEGAL => 'I',
                // sys/brk e falhas de barramento: fora do escopo desta comparação
                Step::Trap(c) if c == cause::SYSCALL => 'S',
                Step::Trap(c) if c == cause::BREAKPOINT => 'B',
                // palavra crua que decodificou como LD/ST para fora da RAM do caso
                Step::Trap(c) if c == cause::BUS_ERROR => 'X',
                other => panic!("passo inesperado {other:?} para a palavra {word}"),
            }
        };
        let after: Vec<i64> = (-13..=13i8).map(|r| m.reg(r).val()).collect();
        let j = |v: &[i64]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(" ");
        writeln!(
            out,
            "{pc} | {} | {maddr} {} | {} || {class} | {} | {} | {}",
            j(&before),
            mval.val(),
            word.val(),
            m.pc.val(),
            j(&after),
            m.peek(maddr).unwrap().val()
        )
        .unwrap();
    }
}
