//! `tir_bench` — mede as fases do caminho T27R → TIR → .tas (docs/13).
//!
//! Uso: cargo run --release --bin tir_bench [funções=2000] [repetições=7]
//!
//! Entrada sintética (a mesma de tests/tir.rs): funções encadeadas com
//! while, match, if, && / || e chamadas. Cada fase é medida isolada, o
//! melhor de N repetições. Os números valem para esta máquina e este dia.

use std::time::{Duration, Instant};

use tenerura_t27::lang::{self, gen, ir, irgen, lower};

fn synth(n: usize) -> String {
    let mut s = String::new();
    for k in 0..n {
        let next = if k + 1 < n { format!("f{}(n - 1)", k + 1) } else { "0".into() };
        s += &format!(
            "fn f{k}(n: word) -> word {{
                let mut i = 0;
                let mut s = {k};
                while i < n && s < 1000000 {{
                    match sign(i - 5) {{
                        -1 => {{ s = s + i; }}
                        0 => {{ s = s * 2; }}
                        _ => {{ if (i % 3 == 0) || !(s > 10) {{ s = s - 1; }} else {{ s = s + 2; }} }}
                    }}
                    i = i + 1;
                }}
                if n <= 0 {{ return s; }}
                return s + {next};
            }}\n"
        );
    }
    s + "fn main() -> word { return f0(3); }\n"
}

fn best<T>(reps: usize, mut f: impl FnMut() -> T) -> (Duration, T) {
    let mut out = None;
    let mut t = Duration::MAX;
    for _ in 0..reps {
        let t0 = Instant::now();
        let r = f();
        t = t.min(t0.elapsed());
        out = Some(r);
    }
    (t, out.unwrap())
}

fn main() {
    let a: Vec<usize> = std::env::args().skip(1).filter_map(|s| s.parse().ok()).collect();
    let (nf, reps) = (a.first().copied().unwrap_or(2000), a.get(1).copied().unwrap_or(7));
    let jobs = ir::jobs();
    let src = synth(nf);

    let (t_front, prog) = best(reps, || lang::front(&src, lang::Mode::User).unwrap_or_else(|d| panic!("{d}")));
    let (t_gen, tas_pilha) = best(reps, || gen::generate(&prog));
    let (t_lower, m) = best(reps, || lower::lower(&prog, lang::Mode::User).unwrap());
    let (t_val1, _) = best(reps, || ir::validate(&m, 1).unwrap());
    let (t_valn, _) = best(reps, || ir::validate(&m, jobs).unwrap());
    let (t_enc, bytes) = best(reps, || m.encode());
    let (t_dec, back) = best(reps, || ir::Module::decode(&bytes).unwrap());
    assert_eq!(back, m);
    let (t_irg1, tas1) = best(reps, || irgen::generate(&m, 1).unwrap());
    let (t_irgn, tasn) = best(reps, || irgen::generate(&m, jobs).unwrap());
    assert_eq!(tas1, tasn, "gerador paralelo diverge do sequencial");
    let text = ir::print(&m);

    let ni = m.insts.len() as f64;
    let ms = |d: Duration| d.as_secs_f64() * 1e3;
    let rate = |d: Duration| ni / d.as_secs_f64() / 1e6;
    println!("entrada: {nf} funções, {} bytes de fonte; {} threads disponíveis; melhor de {reps}", src.len(), jobs);
    println!(
        "TIR: {} instruções, {} regiões, {} palavras extra, {} valores; binário {} bytes ({:.1} B/instr); texto {} bytes",
        m.insts.len(),
        m.regions.len(),
        m.extra.len(),
        m.funcs.iter().map(|f| f.nvals as u64).sum::<u64>(),
        bytes.len(),
        bytes.len() as f64 / ni,
        text.len()
    );
    println!("{:<34} {:>9} {:>14}", "fase", "ms", "M instr TIR/s");
    for (name, d) in [
        ("fonte → AST checada (lex+parse+check)", t_front),
        ("AST → .tas  (gerador de pilha)", t_gen),
        ("AST → TIR   (lower, 1 passada)", t_lower),
        ("validação, 1 thread", t_val1),
        (&*format!("validação, {jobs} threads"), t_valn),
        ("encode", t_enc),
        ("decode", t_dec),
        ("TIR → .tas, 1 thread", t_irg1),
        (&*format!("TIR → .tas, {jobs} threads"), t_irgn),
    ] {
        println!("{name:<34} {:>9.3} {:>14.1}", ms(d), rate(d));
    }
    println!(
        "decode: {:.0} MB/s; .tas: pilha {} linhas, tir {} linhas",
        bytes.len() as f64 / t_dec.as_secs_f64() / 1e6,
        tas_pilha.lines().count(),
        tas1.lines().count()
    );
}
