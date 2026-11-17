//! TENERURA — benchmark comparativo das representações (§4.2 e §6 do plano).
//!
//! Compara três abordagens para a MESMA operação (soma dos valores dos trits):
//!   A. processamento direto sobre buffer 2-bits (4 trits/byte, decodificação por nibble)
//!   B. desempacotamento 2-bits → Vec<i8> → soma
//!   C. processamento de palavras densas base-3 (n=20 trits em u32)
//!
//! Metodologia honesta: 1 aquecimento + 5 medições por abordagem; relata média
//! e o mínimo (min = estimativa menos ruidosa do custo estável); 1 única
//! máquina, 1 única rodada — NÃO é uma conclusão de desempenho universal.
//! Resultado bruto vai para results/fase2-rust-bench.txt.

use std::time::Instant;

use tenerura_trits::pack2;
use tenerura_trits::pack3;
use tenerura_trits::trit::{s3_of_val, ArithTrit};

const N_TRITS: usize = 1_000_000;
const WARMUP: usize = 1;
const REPS: usize = 5;

/// Soma com decodificação por nibble diretamente no buffer 2-bits.
/// Usa desdobramento simples: cada byte → 4 trits via match em nibbles.
fn bench_a_packed2(buf: &[u8]) -> i64 {
    let mut sum: i64 = 0;
    for &b in buf {
        // 4 nibbles por byte
        for shift in [0u32, 2, 4, 6] {
            match (b >> shift) & 0b11 {
                0b01 => sum += 1,
                0b10 => sum -= 1,
                _ => {}
            }
        }
    }
    sum
}

/// Desempacota todo o buffer para Vec<i8> e depois soma.
fn bench_b_unpack(buf: &[u8]) -> i64 {
    let unpacked: Vec<i8> = (0..N_TRITS)
        .map(|i| pack2::get_trit(buf, i).unwrap().val())
        .collect();
    unpacked.iter().map(|&v| v as i64).sum()
}

/// Processamento DIRETO sobre palavras densas base-3 (n=20 em u32):
/// extração de dígitos por divisão, sem alocação.
fn bench_c_dense(words: &[u32]) -> i64 {
    let mut sum: i64 = 0;
    for &w in words {
        let mut x = w;
        for _ in 0..20 {
            let u = x % 3;
            x /= 3;
            sum += u as i64 - 1; // dígito não-balanceado → balanceado
        }
    }
    sum
}

fn measure(name: &str, bytes_trit: f64, f: impl Fn() -> i64) -> String {
    let mut sum = f(); // aquecimento; guarda o resultado esperado
    let mut times = Vec::with_capacity(REPS);
    for _ in 0..REPS {
        let t0 = Instant::now();
        let r = f();
        times.push(t0.elapsed());
        assert_eq!(r, sum, "resultado inconsistente entre rodadas de {name}");
    }
    let avg = times.iter().sum::<std::time::Duration>() / REPS as u32;
    let min = times.iter().min().unwrap();
    let throughput = (N_TRITS as f64) / (avg.as_secs_f64()) / 1e6;
    format!(
        "{name}: soma={sum} media={:.2?} min={:.2?} trits/s={:.1}M bytes/trit={:.3}",
        avg, min, throughput, bytes_trit
    )
}

fn main() {
    // Dados: PRNG xorshift64*, semente registrada (mesma do teste de propriedade)
    let mut state = 0x5EED_2026_1116u64;
    let mut next = move || {
        state ^= state >> 12;
        state ^= state << 25;
        state ^= state >> 27;
        state.wrapping_mul(0x2545_F491_4F6C_DD1D)
    };

    // Buffer 2-bits: 4 trits por byte
    let mut buf2 = vec![0u8; N_TRITS / 4];
    for i in 0..N_TRITS {
        let d = (next() % 3) as i8 - 1;
        pack2::set_trit(&mut buf2, i, ArithTrit(s3_of_val(d)));
    }
    // Palavras densas: 20 trits por u32
    let n_words = N_TRITS / 20;
    let mut words = Vec::with_capacity(n_words);
    for _ in 0..n_words {
        let ds: Vec<i8> = (0..20).map(|_| (next() % 3) as i8 - 1).collect();
        words.push(pack3::pack3_u32(&ds).unwrap());
    }

    println!(
        "TENERURA bench — N={} trits, aquecimento=1 rodada(s), {} medições, LTO, release",
        N_TRITS, REPS
    );
    println!(
        "tamanhos: buf2={} bytes (2 bits/trit) | words={} x u32 (32 bits/20 trits = 1.6 bytes/trit)",
        buf2.len(),
        n_words
    );

    let a = measure("A soma-direta-2bits ", 0.25, || bench_a_packed2(&buf2));
    let b = measure("B desempacota+soma  ", 0.25, || bench_b_unpack(&buf2));
    let c = measure("C denso-base3-n20   ", 0.2, || bench_c_dense(&words));
    println!("{a}");
    println!("{b}");
    println!("{c}");
    println!("NOTA: single-machine, single-run; somas de A e B devem conferir (mesmos dados);");
    println!("      C soma conjunto próprio (soma esperada ~0 por simetria dos dados).");
    println!("      bytes/trit: A/B = 2 bits/trit = 0,25; C = 1,6 bits/trit = 0,20 (ideal 1,585).");
}
