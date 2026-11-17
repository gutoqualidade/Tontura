//! TENERURA Fase 5 — medições (results/phase5/bench.txt).
//!
//! E1. Representação da palavra de 27 trits no EMULADOR: valor em `i64`
//!     (a adotada) × dois planos de bits (2 bits/trit, como o pack2 da
//!     Fase 2) × vetor de dígitos `[i8; 27]` × `Vec<i8>` (a ALU auditada).
//! E2. Custo de separar página/deslocamento: base 3 pura (página de 3⁶
//!     palavras) × tamanhos "amigos do binário" — no hospedeiro (ns) e num
//!     modelo de hardware ternário (avaliações de célula somadora).
//! E3. Vazão do emulador numa carga real (núcleo + tarefas) e custo da
//!     decodificação.
//!
//! Metodologia (a mesma da Fase 2): 1 aquecimento + 5 medições; relata
//! mínimo e média; UMA máquina, UMA rodada — não é conclusão universal.
//! Uso: cargo run --release --example bench

use std::hint::black_box;
use std::path::PathBuf;
use std::time::Instant;

use tenerura_t27::asm::assemble_file;
use tenerura_t27::isa::{decode, encode, Fmt, Instr, Op, IMM16_MAX, IMM19_MAX, LUI_MAX};
use tenerura_t27::machine::{page_of, Machine, Stop, DEFAULT_RAM_HALF, PAGE_SIZE};
use tenerura_t27::manifest::load_manifest;
use tenerura_t27::word::{Word, POW3, TRITS};
use tenerura_trits::alu;

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
}

const SEED: u64 = 0x5EED_2026_1116 ^ 0xBE4C;
const N: usize = 1 << 16;
const REPS: usize = 5;

/// Mede `f` (que processa os N pares) e devolve (ns/op mínimo, ns/op médio).
fn measure(mut f: impl FnMut() -> i64) -> (f64, f64) {
    black_box(f());
    let mut times = Vec::new();
    for _ in 0..REPS {
        let t = Instant::now();
        black_box(f());
        times.push(t.elapsed().as_nanos() as f64 / N as f64);
    }
    let min = times.iter().cloned().fold(f64::MAX, f64::min);
    (min, times.iter().sum::<f64>() / REPS as f64)
}

// ───────── representação B: dois planos de bits (bit i = trit i) ─────────

#[derive(Clone, Copy)]
struct Planes {
    p: u32,
    n: u32,
}
const MASK: u32 = (1 << 27) - 1;

impl Planes {
    fn from_word(w: Word) -> Planes {
        let (mut p, mut n) = (0u32, 0u32);
        for (i, &d) in w.trits().iter().enumerate() {
            if d > 0 {
                p |= 1 << i;
            } else if d < 0 {
                n |= 1 << i;
            }
        }
        Planes { p, n }
    }
    /// valor inteiro (necessário para usar a palavra como endereço)
    fn value(self) -> i64 {
        let mut v = 0i64;
        for i in (0..27).rev() {
            v = v * 3 + ((self.p >> i) & 1) as i64 - ((self.n >> i) & 1) as i64;
        }
        v
    }
    /// soma por propagação de carry em paralelo de bits (laço até o carry sumir)
    #[inline]
    fn add(self, o: Planes) -> Planes {
        let (mut a, mut b) = (self, o);
        loop {
            let cp = a.p & b.p;
            let cn = a.n & b.n;
            let s_p = (a.p & !b.p & !b.n) | (b.p & !a.p & !a.n);
            let s_n = (a.n & !b.n & !b.p) | (b.n & !a.n & !a.p);
            let d = Planes { p: s_p | cn, n: s_n | cp };
            let c = Planes { p: (cp << 1) & MASK, n: (cn << 1) & MASK };
            if c.p | c.n == 0 {
                return d;
            }
            a = d;
            b = c;
        }
    }
    #[inline]
    fn kmin(self, o: Planes) -> Planes {
        Planes { p: self.p & o.p, n: self.n | o.n }
    }
    #[inline]
    fn cmp(self, o: Planes) -> i8 {
        let diff = (self.p ^ o.p) | (self.n ^ o.n);
        if diff == 0 {
            return 0;
        }
        let top = 31 - diff.leading_zeros();
        let da = ((self.p >> top) & 1) as i8 - ((self.n >> top) & 1) as i8;
        let db = ((o.p >> top) & 1) as i8 - ((o.n >> top) & 1) as i8;
        if da > db {
            1
        } else {
            -1
        }
    }
    #[inline]
    fn shift(self, k: i32) -> Planes {
        if k >= 0 {
            Planes { p: (self.p << k) & MASK, n: (self.n << k) & MASK }
        } else {
            Planes { p: self.p >> -k, n: self.n >> -k }
        }
    }
}

// ───────── representação C: vetor de dígitos sem alocação ─────────

type Digits = [i8; TRITS];

#[inline]
fn dig_add(a: &Digits, b: &Digits) -> Digits {
    let mut out = [0i8; TRITS];
    let mut c = 0i8;
    for i in 0..TRITS {
        let s = a[i] + b[i] + c;
        let (d, nc) = if s > 1 {
            (s - 3, 1)
        } else if s < -1 {
            (s + 3, -1)
        } else {
            (s, 0)
        };
        out[i] = d;
        c = nc;
    }
    out
}

#[inline]
fn dig_cmp(a: &Digits, b: &Digits) -> i8 {
    for i in (0..TRITS).rev() {
        if a[i] != b[i] {
            return if a[i] > b[i] { 1 } else { -1 };
        }
    }
    0
}

fn os_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../os")
}

fn main() {
    let mut rng = XorShift(SEED);
    let words: Vec<Word> = (0..2 * N).map(|_| Word::wrap128(rng.next() as i128)).collect();
    let (wa, wb) = words.split_at(N);
    let pa: Vec<Planes> = wa.iter().map(|&w| Planes::from_word(w)).collect();
    let pb: Vec<Planes> = wb.iter().map(|&w| Planes::from_word(w)).collect();
    let da: Vec<Digits> = wa.iter().map(|w| w.trits()).collect();
    let db: Vec<Digits> = wb.iter().map(|w| w.trits()).collect();
    let va: Vec<Vec<i8>> = da.iter().map(|d| d.to_vec()).collect();
    let vb: Vec<Vec<i8>> = db.iter().map(|d| d.to_vec()).collect();
    let ks: Vec<i32> = (0..N).map(|_| (rng.next() % 53) as i32 - 26).collect();

    // conferência: as quatro representações calculam a MESMA soma
    for i in 0..N {
        let s = wa[i].add(wb[i]).0;
        assert_eq!(pa[i].add(pb[i]).value(), s.val());
        assert_eq!(dig_add(&da[i], &db[i]), s.trits());
        assert_eq!(alu::add(&va[i], &vb[i], TRITS).0, s.trits().to_vec());
        assert_eq!(pa[i].cmp(pb[i]), wa[i].cmp3(wb[i]));
        assert_eq!(pa[i].kmin(pb[i]).value(), wa[i].kmin(wb[i]).val());
        assert_eq!(pa[i].shift(ks[i]).value(), wa[i].shift(ks[i] as i64).val());
        assert_eq!(pa[i].value(), wa[i].val());
    }

    let mut out = String::new();
    let mut line = |s: String| {
        println!("{s}");
        out.push_str(&s);
        out.push('\n');
    };
    line("TENERURA Fase 5 — medições do emulador (uma máquina, uma rodada; ns por operação: mínimo / média de 5)".into());
    line(format!("semente {SEED:#x}; {N} pares de palavras de 27 trits uniformes na janela"));
    line(String::new());
    line("E1 — representação da palavra (27 trits)".into());
    line(format!("{:<34} {:>14} {:>14} {:>14} {:>14}", "operação", "i64 (valor)", "2 planos", "[i8;27]", "Vec<i8> (ALU)"));

    let fmt = |m: (f64, f64)| format!("{:.2} / {:.2}", m.0, m.1);
    let na = "—".to_string();
    // soma
    let r_i = measure(|| (0..N).fold(0i64, |acc, i| acc ^ wa[i].add(wb[i]).0.val()));
    let r_p = measure(|| (0..N).fold(0i64, |acc, i| acc ^ pa[i].add(pb[i]).p as i64));
    let r_d = measure(|| (0..N).fold(0i64, |acc, i| acc ^ dig_add(&da[i], &db[i])[13] as i64));
    let r_v = measure(|| (0..N).fold(0i64, |acc, i| acc ^ alu::add(&va[i], &vb[i], TRITS).0[13] as i64));
    line(format!("{:<34} {:>14} {:>14} {:>14} {:>14}", "soma (add)", fmt(r_i), fmt(r_p), fmt(r_d), fmt(r_v)));
    let add_cost = (r_i.0, r_p.0, r_d.0, r_v.0);
    // comparação
    let r_i = measure(|| (0..N).fold(0i64, |acc, i| acc + wa[i].cmp3(wb[i]) as i64));
    let r_p = measure(|| (0..N).fold(0i64, |acc, i| acc + pa[i].cmp(pb[i]) as i64));
    let r_d = measure(|| (0..N).fold(0i64, |acc, i| acc + dig_cmp(&da[i], &db[i]) as i64));
    let r_v = measure(|| (0..N).fold(0i64, |acc, i| acc + alu::cmp(&va[i], &vb[i]) as i64));
    line(format!("{:<34} {:>14} {:>14} {:>14} {:>14}", "comparação (cmp)", fmt(r_i), fmt(r_p), fmt(r_d), fmt(r_v)));
    let cmp_cost = (r_i.0, r_p.0, r_d.0, r_v.0);
    // multiplicação
    let r_i = measure(|| (0..N).fold(0i64, |acc, i| acc ^ wa[i].mul(wb[i]).0.val()));
    let r_v = measure(|| (0..N).fold(0i64, |acc, i| acc ^ alu::mul(&va[i], &vb[i], TRITS).0[13] as i64));
    line(format!("{:<34} {:>14} {:>14} {:>14} {:>14}", "multiplicação (mul, palavra baixa)", fmt(r_i), na, na, fmt(r_v)));
    // deslocamento
    let r_i = measure(|| (0..N).fold(0i64, |acc, i| acc ^ wa[i].shift(ks[i] as i64).val()));
    let r_p = measure(|| (0..N).fold(0i64, |acc, i| acc ^ pa[i].shift(ks[i]).p as i64));
    line(format!("{:<34} {:>14} {:>14} {:>14} {:>14}", "deslocamento por k ∈ [−26, 26]", fmt(r_i), fmt(r_p), na, na));
    // lógica trit a trit
    let r_i = measure(|| (0..N).fold(0i64, |acc, i| acc ^ wa[i].kmin(wb[i]).val()));
    let r_p = measure(|| (0..N).fold(0i64, |acc, i| acc ^ pa[i].kmin(pb[i]).p as i64));
    line(format!("{:<34} {:>14} {:>14} {:>14} {:>14}", "mínimo trit a trit (kmin)", fmt(r_i), fmt(r_p), na, na));
    // palavra → índice inteiro (todo acesso à memória precisa disto)
    let r_i = measure(|| (0..N).fold(0i64, |acc, i| acc ^ wa[i].val()));
    let r_p = measure(|| (0..N).fold(0i64, |acc, i| acc ^ pa[i].value()));
    let r_d = measure(|| (0..N).fold(0i64, |acc, i| acc ^ Word::from_trits(&da[i]).unwrap().val()));
    line(format!("{:<34} {:>14} {:>14} {:>14} {:>14}", "palavra → índice (endereço)", fmt(r_i), fmt(r_p), fmt(r_d), na));
    let idx_cost = (r_i.0, r_p.0, r_d.0);

    // ───────── E3: carga real, mistura de operações, vazão ─────────
    line(String::new());
    line("E3 — emulador numa carga real (núcleo v1 + demonstração 2 com entrada, e demonstração 1)".into());
    let run_demo = |name: &str, input: &str, steps: u64| -> Machine {
        let img = load_manifest(&os_dir().join(format!("sys/{name}.sys"))).expect("manifesto");
        let mut m = Machine::new(DEFAULT_RAM_HALF);
        img.load(&mut m);
        let mut stop = m.run(steps);
        if stop == Stop::Idle {
            m.console_in.extend(input.chars().map(|c| c as i64));
            stop = m.run(steps);
        }
        black_box(stop);
        m
    };
    let mut best = f64::MAX;
    let mut total_steps = 0i64;
    for _ in 0..=REPS {
        let t = Instant::now();
        let m1 = run_demo("demo1", "", 20_000_000);
        let m2 = run_demo("demo2", "ola ternura q", 20_000_000);
        let dt = t.elapsed().as_secs_f64();
        total_steps = m1.time + m2.time;
        best = best.min(dt);
    }
    line(format!(
        "vazão: {:.1} milhões de passos/s (melhor de {}; {} passos por rodada)",
        total_steps as f64 / best / 1e6,
        REPS + 1,
        total_steps
    ));
    let m = run_demo("demo2", "ola ternura q", 20_000_000);
    let total: u64 = m.stats.ops.iter().sum();
    let share = |ops: &[Op]| ops.iter().map(|&o| m.stats.ops[o as usize]).sum::<u64>() as f64 / total as f64;
    let arith = share(&[Op::Add, Op::Sub, Op::Addi, Op::Addc]);
    let branch = share(&[Op::Beq, Op::Bne, Op::Blt, Op::Bge, Op::Bgt, Op::Ble]);
    let mem = share(&[Op::Ld, Op::St]);
    let muldiv = share(&[Op::Mul, Op::Mulh, Op::Div, Op::Rem]);
    let shifts = share(&[Op::Sh, Op::Shi]);
    let logic = share(&[Op::Kmin, Op::Kmax]);
    line(format!(
        "mistura dinâmica (demo 2, {} instruções): soma/sub {:.1}%  desvio {:.1}%  memória {:.1}%  desloc. {:.1}%  mul/div {:.2}%  lógica trit a trit {:.2}%  outras {:.1}%",
        total,
        arith * 100.0,
        branch * 100.0,
        mem * 100.0,
        shifts * 100.0,
        muldiv * 100.0,
        logic * 100.0,
        (1.0 - arith - branch - mem - shifts - muldiv - logic) * 100.0
    ));
    // custo por instrução atribuível à representação: toda instrução usa o PC
    // como índice (1 conversão); memória usa mais 1; soma e desvio usam add/cmp
    let weighted = |add: f64, cmp: f64, idx: f64| arith * add + branch * cmp + (1.0 + mem) * idx;
    line(format!(
        "custo ponderado por instrução só de aritmética+índices: i64 {:.2} ns | 2 planos {:.2} ns | [i8;27] {:.2} ns",
        weighted(add_cost.0, cmp_cost.0, idx_cost.0),
        weighted(add_cost.1, cmp_cost.1, idx_cost.1),
        weighted(add_cost.2, cmp_cost.2, idx_cost.2),
    ));

    // decodificação
    let mut rng = XorShift(SEED ^ 0xDEC0);
    let instrs: Vec<Word> = (0..N)
        .map(|_| {
            let op = Op::ALL[(rng.next() % Op::ALL.len() as u64) as usize];
            let r = |rng: &mut XorShift| (rng.next() % 27) as i8 - 13;
            let im = |rng: &mut XorShift, max: i64| (rng.next() % (2 * max as u64 + 1)) as i64 - max;
            let ins = match op.fmt() {
                Fmt::R => Instr::r(op, r(&mut rng), r(&mut rng), r(&mut rng)),
                Fmt::I => match op {
                    Op::Lui => Instr::i(op, r(&mut rng), 0, im(&mut rng, LUI_MAX)),
                    Op::Csrr => Instr::i(op, r(&mut rng), 0, 1),
                    Op::Csrw => Instr::i(op, 0, r(&mut rng), 1),
                    _ => Instr::i(op, r(&mut rng), r(&mut rng), im(&mut rng, IMM16_MAX)),
                },
                Fmt::J => Instr::j(op, r(&mut rng), im(&mut rng, IMM19_MAX)),
                Fmt::N => Instr::n(op),
            };
            encode(&ins).unwrap()
        })
        .collect();
    let r = measure(|| (0..N).fold(0i64, |acc, i| acc ^ decode(instrs[i]).map(|x| x.imm).unwrap_or(0)));
    line(format!("decodificação de instrução (campos por divisão por potências de 3): {} ns", fmt(r)));

    // ───────── E2: separar página / deslocamento ─────────
    line(String::new());
    line("E2 — separar endereço em (página, deslocamento)".into());
    let addrs: Vec<i64> = wa.iter().map(|w| w.val() % 190_000_000).collect();
    let r3 = measure(|| {
        (0..N).fold(0i64, |acc, i| {
            let p = page_of(addrs[i]);
            acc ^ p ^ (addrs[i] - p * PAGE_SIZE)
        })
    });
    let r2 = measure(|| (0..N).fold(0i64, |acc, i| acc ^ (addrs[i] >> 10) ^ (addrs[i] & 1023)));
    let r10 = measure(|| (0..N).fold(0i64, |acc, i| acc ^ addrs[i].div_euclid(1000) ^ addrs[i].rem_euclid(1000)));
    line(format!("no hospedeiro binário:  página 3⁶ = 729 (centrada): {} ns | página 2¹⁰ = 1024 (máscara): {} ns | página 1000: {} ns", fmt(r3), fmt(r2), fmt(r10)));
    // modelo de hardware ternário: quantas células somadoras de 1 trit são
    // avaliadas para obter (página, deslocamento) de um endereço de 27 trits
    let cells_pow3 = 0; // potência de 3: os dígitos JÁ são os campos (só fiação)
    let cells = |divisor: i64| {
        // divisão longa em base 3, dígito a dígito (restauradora): a cada um
        // dos 27 passos, até 2 somas/subtrações de largura = dígitos do divisor + 1
        let width = (1..28).find(|&k| POW3[k] > 2 * divisor).unwrap() as i64 + 1;
        27 * 2 * width
    };
    line(format!(
        "em hardware ternário (modelo: células somadoras de 1 trit avaliadas): página 3⁶: {} (os campos são os próprios dígitos) | página 1024: {} | página 1000: {}",
        cells_pow3,
        cells(1024),
        cells(1000)
    ));
    line("  (modelo de contagem sobre divisão longa restauradora; não é síntese de circuito)".into());

    // tamanhos de artefatos
    line(String::new());
    let k = assemble_file(&os_dir().join("kernel/kernel.tas")).unwrap();
    line(format!(
        "núcleo: {} palavras de código, {} de dados estáticos; Word = {} bytes no emulador ({} bits úteis de 64)",
        k.text().words.len(),
        k.data().map_or(0, |d| d.words.len()),
        std::mem::size_of::<Word>(),
        "42,8"
    ));
    if std::env::var("T27_WRITE_RESULTS").is_ok() {
        std::fs::write(os_dir().join("../../results/phase5/bench.txt"), &out).unwrap();
    }
}
