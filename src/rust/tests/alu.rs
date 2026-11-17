//! TENERURA Fase 4 — testes da ALU ternária (§7.4 do plano).
//!
//! Estratégia:
//! - Exaustivo para n ≤ 3 (27 palavras; 27³ pares para add/cmp).
//! - Propriedades com PRNG xorshift64*, semente 0x5EED_2026_1116 (registrada),
//!   para n ∈ {5, 9, 20, 27, 40}.
//! - DIFERENCIAL contra referência i128 independente (aritmética exata):
//!   a ALU e a referência não compartilham algoritmo (schoolbook vs inteiro).
//! - A6 (truncamento = arredondamento sem empates) testado em shr/from_i64.
//! - Overflow testado explicitamente (add/shl/mul) contra a referência.

use tenerura_trits::alu;
use tenerura_trits::pack3;

/// PRNG xorshift64* — mesma construção da Fase 2 (semente registrada).
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

    fn digit(&mut self) -> i8 {
        match self.next() % 3 {
            0 => -1,
            1 => 0,
            _ => 1,
        }
    }

    /// i64 uniforme na faixa [−M, M] da largura n.
    fn in_range(&mut self, n: usize) -> i64 {
        let m = alu::m_max(n);
        let span = (2 * m as i128 + 1) as u64; // 2M+1 = 3ⁿ ≤ u64::MAX (n≤40)
        // r − m ∈ [−M, M] cabe em i64, mas o cálculo intermediário precisa de i128
        ((self.next() % span) as i128 - m as i128) as i64
    }
}

const SEED: u64 = 0x5EED_2026_1116;

fn rand_word(rng: &mut XorShift, n: usize) -> Vec<i8> {
    (0..n).map(|_| rng.digit()).collect()
}

// ---------------- Exaustivo n ≤ 3 ----------------

#[test]
fn exhaustive_add_sub_cmp_n3() {
    let n = 3;
    let vals: Vec<i64> = (-13..=13).collect();
    for &a in &vals {
        for &b in &vals {
            let wa = alu::from_i64(a, n);
            let wb = alu::from_i64(b, n);
            // soma com referência exata
            let (w, ovf) = alu::add(&wa, &wb, n);
            let s = a + b;
            assert_eq!(ovf, s.abs() > 13, "add flag {a}+{b}");
            let wrapped = if s > 13 {
                s - 27
            } else if s < -13 {
                s + 27
            } else {
                s
            };
            assert_eq!(pack3::bval(&w), wrapped, "add {a}+{b}");
            // subtração
            let (wd, ovfd) = alu::sub(&wa, &wb, n);
            let d = a - b;
            assert_eq!(ovfd, d.abs() > 13, "sub flag {a}-{b}");
            let wrapped_d = if d > 13 {
                d - 27
            } else if d < -13 {
                d + 27
            } else {
                d
            };
            assert_eq!(pack3::bval(&wd), wrapped_d, "sub {a}-{b}");
            // comparação
            let c = alu::cmp(&wa, &wb);
            let expected = if a < b { -1 } else if a > b { 1 } else { 0 };
            assert_eq!(c, expected, "cmp {a} vs {b}");
        }
    }
}

#[test]
fn exhaustive_neg_n3() {
    for v in -13..=13 {
        let w = alu::from_i64(v, 3);
        assert_eq!(pack3::bval(&alu::neg(&w)), -v);
        assert_eq!(alu::neg(&alu::neg(&w)), w); // involução
    }
}

#[test]
fn exhaustive_shr_shl_n3() {
    for v in -13..=13 {
        let w = alu::from_i64(v, 3);
        // shr: ÷3 arredondado (sem empates)
        let q = {
            let base = v.div_euclid(3);
            let r = v.rem_euclid(3);
            if r >= 2 {
                base + 1
            } else {
                base
            }
        };
        assert_eq!(pack3::bval(&alu::shr(&w)), q, "shr {v}");
        // shl: ×3, overflow ⟺ topo ≠ 0
        let (ws, ovf) = alu::shl(&w);
        assert_eq!(ovf, w[2] != 0, "shl {v}");
        if !ovf {
            assert_eq!(pack3::bval(&ws), 3 * v, "shl {v}");
        }
    }
}

#[test]
fn exhaustive_mul_n2_and_flags() {
    for a in -4..=4 {
        for b in -4..=4 {
            let (w, ovf) = alu::mul(&alu::from_i64(a, 2), &alu::from_i64(b, 2), 2);
            let p = a * b;
            assert_eq!(ovf, p.abs() > 4, "mul flag {a}·{b}");
            assert_eq!((pack3::bval(&w) - p).rem_euclid(9), 0, "mul {a}·{b}");
            assert!(pack3::bval(&w).abs() <= 4);
        }
    }
}

// ---------------- Propriedades com PRNG (larguras grandes) ----------------

const WIDTHS: [usize; 5] = [5, 9, 20, 27, 40];
const ITERS: usize = 20_000;

#[test]
fn property_add_vs_i128_reference() {
    let mut rng = XorShift(SEED);
    for &n in &WIDTHS {
        for _ in 0..ITERS {
            let wa = rand_word(&mut rng, n);
            let wb = rand_word(&mut rng, n);
            let va = pack3::bval(&wa) as i128;
            let vb = pack3::bval(&wb) as i128;
            let s = va + vb;
            let m = alu::m_max(n) as i128;
            let (w, ovf) = alu::add(&wa, &wb, n);
            assert_eq!(ovf, s.abs() > m, "add flag n={n}");
            // wrap: w ≡ s (mod 3ⁿ) e |bval(w)| ≤ M
            let p3: i128 = {
                let mut p = 1;
                for _ in 0..n {
                    p *= 3;
                }
                p
            };
            assert_eq!((pack3::bval(&w) as i128 - s).rem_euclid(p3), 0, "add wrap n={n}");
            assert!(pack3::bval(&w).abs() <= m as i64, "add faixa n={n}");
        }
    }
}

#[test]
fn property_mul_vs_i128_reference() {
    let mut rng = XorShift(SEED ^ 0x3B1A);
    for &n in &WIDTHS[..4] {
        // n=40: produto ~3.7e37 ok em i128, mas mantemos 4 larguras por custo
        for _ in 0..ITERS / 2 {
            let wa = rand_word(&mut rng, n);
            let wb = rand_word(&mut rng, n);
            let p = pack3::bval(&wa) as i128 * pack3::bval(&wb) as i128;
            let m = alu::m_max(n) as i128;
            let (w, ovf) = alu::mul(&wa, &wb, n);
            assert_eq!(ovf, p.abs() > m, "mul flag n={n}");
            let p3: i128 = {
                let mut q = 1;
                for _ in 0..n {
                    q *= 3;
                }
                q
            };
            assert_eq!((pack3::bval(&w) as i128 - p).rem_euclid(p3), 0, "mul wrap n={n}");
            assert!(pack3::bval(&w).abs() <= m as i64);
        }
    }
}

#[test]
fn property_neg_involutive_and_sub() {
    let mut rng = XorShift(SEED ^ 0x9E61);
    for &n in &WIDTHS {
        for _ in 0..ITERS {
            let wa = rand_word(&mut rng, n);
            let wb = rand_word(&mut rng, n);
            // involução
            assert_eq!(alu::neg(&alu::neg(&wa)), wa, "neg² n={n}");
            // valor
            assert_eq!(pack3::bval(&alu::neg(&wa)), -pack3::bval(&wa), "neg valor n={n}");
            // sub(a,b) e add(a, neg(b)) são a MESMA função por construção —
            // validamos contra a referência, não entre si:
            let (wd, _) = alu::sub(&wa, &wb, n);
            let d = pack3::bval(&wa) as i128 - pack3::bval(&wb) as i128;
            let p3: i128 = {
                let mut p = 1;
                for _ in 0..n {
                    p *= 3;
                }
                p
            };
            assert_eq!(
                (pack3::bval(&wd) as i128 - d).rem_euclid(p3),
                0,
                "sub wrap n={n}"
            );
        }
    }
}

#[test]
fn property_shr_is_round_to_nearest_no_ties() {
    let mut rng = XorShift(SEED ^ 0x5A16);
    for &n in &WIDTHS {
        for _ in 0..ITERS {
            let v = rng.in_range(n);
            let w = alu::from_i64(v, n);
            let q = pack3::bval(&alu::shr(&w));
            // referência independente: arredondamento ao mais próximo de v/3
            // (sem empates pois 3 é ímpar: v/3 = k ± 1/3 ou k)
            let expected = {
                let base = v.div_euclid(3);
                let r = v.rem_euclid(3);
                if r >= 2 {
                    base + 1
                } else {
                    base
                }
            };
            assert_eq!(q, expected, "shr n={n} v={v}");
        }
    }
}

#[test]
fn property_shl_overflow_iff_top() {
    let mut rng = XorShift(SEED ^ 0x5A10);
    for &n in &WIDTHS {
        for _ in 0..ITERS {
            let wa = rand_word(&mut rng, n);
            let (ws, ovf) = alu::shl(&wa);
            assert_eq!(ovf, wa[n - 1] != 0, "shl flag n={n}");
            if !ovf {
                assert_eq!(pack3::bval(&ws), 3 * pack3::bval(&wa), "shl valor n={n}");
            }
        }
    }
}

#[test]
fn property_from_i64_window_congruence_uniqueness() {
    // A6 decomposto em seus conteúdos verdadeiros:
    //   (i)  q = from_i64(v,n) ≡ v (mod 3ⁿ) e q ∈ [−M, M];
    //   (ii) UNICIDADE: q é a única representante da classe na janela
    //        (q ± 3ⁿ estão fora — janela de largura exatamente 3ⁿ);
    //   (iii) quociente arredondado: k = (v−q)/3ⁿ = round(v/3ⁿ) e NUNCA
    //        há empate (2·r = 3ⁿ impossível, 3ⁿ ímpar).
    let mut rng = XorShift(SEED ^ 0xA6A6);
    let n = 9;
    let p3: i64 = 19_683;
    let m = alu::m_max(n);
    for _ in 0..ITERS {
        let v = rng.in_range(n) + rng.in_range(n); // |v| ≤ 2M (fora da janela)
        let q = pack3::bval(&alu::from_i64(v, n));
        assert_eq!((q - v).rem_euclid(p3), 0, "congruência v={v}");
        assert!(q.abs() <= m, "janela v={v} q={q}");
        assert!(q.abs() + p3 > m, "unicidade inferior v={v} q={q}");
        let k = (v - q).div_euclid(p3);
        let k0 = v.div_euclid(p3);
        let r = v.rem_euclid(p3);
        assert_ne!(2 * r, p3, "empate impossível v={v}");
        let k_expected = if 2 * r > p3 { k0 + 1 } else { k0 };
        assert_eq!(k, k_expected, "quociente arredondado v={v}");
    }
}

#[test]
fn property_sign_extension_cross_width() {
    let mut rng = XorShift(SEED ^ 0xE474);
    for _ in 0..ITERS {
        let v = rng.in_range(9);
        let w9 = alu::from_i64(v, 9);
        let w20 = alu::width_extend(&w9, 9, 20);
        assert_eq!(pack3::bval(&w20), v, "sign ext 9→20 v={v}");
        assert_eq!(alu::from_i64(v, 20), w20, "canonicidade 9→20 v={v}");
        // redução (truncamento) 20→9 também preserva via mod balanceado
        let back = alu::from_i64(pack3::bval(&w20), 9);
        assert_eq!(pack3::bval(&back), v);
    }
}

#[test]
fn property_cmp_matches_reference() {
    let mut rng = XorShift(SEED ^ 0xC0D9);
    for &n in &WIDTHS {
        for _ in 0..ITERS {
            let wa = rand_word(&mut rng, n);
            let wb = rand_word(&mut rng, n);
            let va = pack3::bval(&wa);
            let vb = pack3::bval(&wb);
            let expected = if va < vb {
                -1
            } else if va > vb {
                1
            } else {
                0
            };
            assert_eq!(alu::cmp(&wa, &wb), expected, "cmp n={n}");
        }
    }
}

// ---------------- Casos-limite ----------------

#[test]
fn edge_cases() {
    // zero de todas as larguras
    for &n in &WIDTHS {
        let z = vec![0i8; n];
        let (s, ovf) = alu::add(&z, &z, n);
        assert!(!ovf && pack3::bval(&s) == 0);
        assert_eq!(alu::cmp(&z, &z), 0);
    }
    // extremos: M + M estoura (242 → wrap −1); M + 1 estoura (122 → −121);
    // M + (−M) = 0 sem overflow
    let n = 5;
    let m = alu::m_max(n); // 121
    let wm = alu::from_i64(m, n);
    let (w, ovf) = alu::add(&wm, &wm, n);
    assert!(ovf);
    assert_eq!(pack3::bval(&w), -1); // 242 − 243 = −1
    let (w2, ovf2) = alu::add(&wm, &alu::from_i64(1, n), n);
    assert!(ovf2);
    assert_eq!(pack3::bval(&w2), -m); // 122 − 243 = −121
    let (w3, ovf3) = alu::add(&wm, &alu::neg(&wm), n);
    assert!(!ovf3 && pack3::bval(&w3) == 0);
}

// ============================================================
// Fase 4A — testes da auditoria matemática/semântica/diferencial
// ============================================================

/// Referência independente: round(v / 3^k) ao inteiro mais próximo.
/// EMPATE É IMPOSSÍVEL (3^k é ímpar ⇒ 2·r ≠ 3^k) — a função falha se houvesse.
fn round_div_pow3(v: i64, k: usize) -> i64 {
    let mut p: i64 = 1;
    for _ in 0..k {
        p *= 3;
    }
    let base = v.div_euclid(p);
    let r = v.rem_euclid(p);
    assert_ne!(2 * r, p, "empate em v/3^k não deveria existir");
    if 2 * r > p {
        base + 1
    } else {
        base
    }
}

// ---------------- §4.4 Extensão de largura ----------------

/// Exaustivo: extensão preserva valor, canonicidade, domínio e comparação.
#[test]
fn f4a_width_extend_exhaustive_value_cmp_domain() {
    for n in 1..=3usize {
        let mn = alu::m_max(n);
        for m in [n + 1, 5] {
            for v in -mn..=mn {
                let w = alu::from_i64(v, n);
                let ext = alu::width_extend(&w, n, m);
                assert!(alu::valid_digits(&ext), "domínio {v}:{n}→{m}");
                assert_eq!(pack3::bval(&ext), v, "valor {v}:{n}→{m}");
                assert_eq!(alu::from_i64(v, m), ext, "canonicidade {v}:{n}→{m}");
                // cmp preservado (exaustivo total em n ≤ 2)
                if n == 2 {
                    for u in -mn..=mn {
                        let wu = alu::from_i64(u, n);
                        let eu = alu::width_extend(&wu, n, m);
                        assert_eq!(
                            alu::cmp(&ext, &eu),
                            alu::cmp(&w, &wu),
                            "cmp preservado {v} vs {u}"
                        );
                    }
                }
            }
        }
    }
}

/// Refutação demostrada: REPLICAR o trit de topo adiciona t·(3ᵗᵒ−3ᶠʳᵒᵐ)/2 ≠ 0.
/// Zero-extensão e replicação coincidem SOMENTE quando topo = 0.
#[test]
fn f4a_width_extend_replication_refuted() {
    let (n, m) = (2usize, 3usize);
    let delta = (3i64.pow(m as u32) - 3i64.pow(n as u32)) / 2; // 9
    for v in -4i64..=4 {
        let w = alu::from_i64(v, n);
        let t = w[n - 1] as i64;
        let mut rep = w.clone();
        rep.resize(m, w[n - 1]); // replicação do topo
        assert_eq!(pack3::bval(&rep), v + t * delta, "replicação {v}");
        let ext = alu::width_extend(&w, n, m);
        assert_eq!(pack3::bval(&ext), v, "zero-extensão {v}");
        assert_eq!(rep == ext, t == 0, "coincidência ⟺ topo=0 ({v})");
    }
    // exemplo do relatório: [−1,−1] = −4 replicado = −13 ≠ −4
    let w_neg4 = vec![-1i8, -1];
    let mut rep = w_neg4.clone();
    rep.resize(3, -1);
    assert_eq!(pack3::bval(&rep), -13);
}

/// Exemplo canônico do enunciado: 5/3 → 2 (não o truncamento 1); −5/3 → −2
/// (não −1). Verificado na implementação E contra a referência exata.
#[test]
fn f4a_shr_example_5_and_minus5() {
    for (v, expected) in [(5i64, 2i64), (-5, -2)] {
        let w = alu::from_i64(v, 3);
        let got = pack3::bval(&alu::shr(&w));
        assert_eq!(got, expected, "shr({v})");
        assert_eq!(got, round_div_pow3(v, 1), "referência exata {v}");
        // truncamento em direção a zero DIFERE (contrato distinto):
        assert_ne!(got, v / 3, "não é truncamento convencional {v}");
    }
}

/// Reversibilidade shl/shr com condições EXATAS:
///  (a) shr∘shl = identidade sobre dígitos, SEMPRE;
///  (b) shl∘shr zera o LSB: bval = V − d₀;
///  (c) bval(saída de shl) = 3V ⟺ trit de topo = 0 (⟺ !overflow).
#[test]
fn f4a_shl_shr_reversibility_conditions() {
    for v in -13i64..=13 {
        let w = alu::from_i64(v, 3);
        let (ws, ovf) = alu::shl(&w);
        // (a) shr∘shl = id ⟺ !ovf; com overflow, o topo é perdido (→ 0)
        assert_eq!(alu::shr(&ws) == w, !ovf, "shr∘shl id ⟺ topo=0 ({v})");
        if ovf {
            let mut expected = w.clone();
            expected[2] = 0;
            assert_eq!(alu::shr(&ws), expected, "topo perdido {v}");
        }
        // (c)
        assert_eq!(pack3::bval(&ws) == 3 * v, !ovf, "shl representa 3v? {v}");
        // (b)
        let back = alu::shl(&alu::shr(&w)).0;
        assert_eq!(pack3::bval(&back), v - w[0] as i64, "shl∘shr {v}");
        assert_eq!(back, {
            let mut e = w.clone();
            e[0] = 0;
            e
        }, "shl∘shr zera o LSB {v}");
    }
}

// ---------------- §5.5 trunc_low ----------------

/// trunc_low(k) = arredondamento ao múltiplo de 3ᵏ mais próximo, sem empates.
/// Exaustivo em n = 4, k ∈ {1,2,3}; inclui k = 0 (identidade) e k = n (→ 0),
/// e a equivalência trunc_low(_,1) == shr.
#[test]
fn f4a_trunc_low_is_round_to_nearest_multiple() {
    let n = 4usize;
    let m4 = alu::m_max(n); // 40
    for k in 0..=n {
        for v in -m4..=m4 {
            let w = alu::from_i64(v, n);
            let t = alu::trunc_low(&w, k);
            assert_eq!(t.len(), n, "largura preservada");
            assert_eq!(pack3::bval(&t), round_div_pow3(v, k), "trunc {v} k={k}");
        }
    }
    // k = n ⇒ múltiplo de 3ⁿ mais próximo de |v| ≤ Mₙ é 0
    for v in -m4..=m4 {
        let t = alu::trunc_low(&alu::from_i64(v, n), n);
        assert_eq!(pack3::bval(&t), 0);
    }
    // espelho do btTrunc: k=1 ≡ shr
    let mut rng = XorShift(SEED ^ 0x7C1C);
    for _ in 0..2_000 {
        let v = rng.in_range(9);
        let w = alu::from_i64(v, 9);
        assert_eq!(alu::trunc_low(&w, 1), alu::shr(&w));
    }
}

// ---------------- §6.4 multiplicação (álgebra + n = 40) ----------------

#[test]
fn f4a_mul_algebraic_identities_exhaustive_n2() {
    let n = 2usize;
    let zero = vec![0i8; n];
    let one = alu::from_i64(1, n);
    for a in -4i64..=4 {
        let wa = alu::from_i64(a, n);
        // ·0 e ·1
        let (w0, o0) = alu::mul(&wa, &zero, n);
        assert_eq!((w0, o0), (zero.clone(), false), "a·0");
        let (w1, o1) = alu::mul(&wa, &one, n);
        assert_eq!((w1, o1), (wa.clone(), false), "a·1");
        // comutatividade (dígitos)
        for b in -4i64..=4 {
            let wb = alu::from_i64(b, n);
            let ab = alu::mul(&wa, &wb, n).0;
            let ba = alu::mul(&wb, &wa, n).0;
            assert_eq!(ab, ba, "comutatividade {a}·{b}");
        }
    }
    // distributividade: A·(B+C) ≡ A·B + A·C (mod 3ⁿ), flags coerentes
    for a in -4i64..=4 {
        for b in -4i64..=4 {
            for c in -4i64..=4 {
                let (wb, _) = alu::add(&alu::from_i64(b, n), &alu::from_i64(c, n), n);
                let lhs = alu::mul(&alu::from_i64(a, n), &wb, n).0;
                let (ab, _) = alu::mul(&alu::from_i64(a, n), &alu::from_i64(b, n), n);
                let (ac, _) = alu::mul(&alu::from_i64(a, n), &alu::from_i64(c, n), n);
                let (rhs, orc) = alu::add(&ab, &ac, n);
                assert_eq!((pack3::bval(&lhs) - pack3::bval(&rhs)).rem_euclid(9), 0);
                // coerência de flags — só quando B+C não estourou (aí wrap(B+C)
                // = b+c exato). Com overflow de B+C, a identidade vale sobre os
                // valores ENTREGUES (mod 3ⁿ), já verificado acima.
                if (b + c).abs() <= 4 {
                    assert_eq!(
                        alu::mul(&alu::from_i64(a, n), &wb, n).1,
                        (a * (b + c)).abs() > 4,
                        "flag distrib {a}({b}+{c})"
                    );
                }
                let _ = orc; // rhs pode estourar onde o produto não estoura — wrap ≡
            }
        }
    }
}

/// n = 40: limite EXATO do i128 (|M₄₀|² ≈ 3,7e37 < 1,7e38). Produto e flags
/// contra a referência i128 em largura máxima suportada.
#[test]
fn f4a_mul_n40_property() {
    let mut rng = XorShift(SEED ^ 0x40A0);
    let n = 40usize;
    for _ in 0..2_000 {
        let wa = rand_word(&mut rng, n);
        let wb = rand_word(&mut rng, n);
        let p = pack3::bval(&wa) as i128 * pack3::bval(&wb) as i128;
        let m = alu::m_max(n) as i128;
        let (w, ovf) = alu::mul(&wa, &wb, n);
        assert_eq!(ovf, p.abs() > m, "mul flag n=40");
        let p3: i128 = {
            let mut q = 1i128;
            for _ in 0..n {
                q *= 3;
            }
            q
        };
        assert_eq!((pack3::bval(&w) as i128 - p).rem_euclid(p3), 0, "mul wrap n=40");
        assert!(pack3::bval(&w).abs() <= m as i64);
    }
}

// ---------------- §6.2/§6.3 subtração e negação ----------------

/// −(A+B) = (−A)+(−B) e (−A)−B = −(A+B) na largura fixa (com wrap coerente).
/// Exaustivo em n = 2; PRNG em n = 9.
#[test]
fn f4a_neg_distributes_over_add() {
    let n = 2usize;
    for a in -4i64..=4 {
        for b in -4i64..=4 {
            let (ws, os) = alu::add(&alu::from_i64(a, n), &alu::from_i64(b, n), n);
            let lhs = alu::neg(&ws);
            let (rhs, or_) = alu::add(&alu::neg(&alu::from_i64(a, n)), &alu::neg(&alu::from_i64(b, n)), n);
            // ambos ≡ −(a+b) mod 3ⁿ, na janela
            assert_eq!((pack3::bval(&lhs) - (-(a + b))).rem_euclid(9), 0);
            assert_eq!((pack3::bval(&rhs) - (-(a + b))).rem_euclid(9), 0);
            assert!(pack3::bval(&lhs).abs() <= 4 && pack3::bval(&rhs).abs() <= 4);
            assert_eq!(os, or_, "flags coerentes"); // |a+b|>4 ⟺ |−a−b|>4
        }
    }
    let mut rng = XorShift(SEED ^ 0x9E9E);
    for _ in 0..5_000 {
        let wa = rand_word(&mut rng, 9);
        let wb = rand_word(&mut rng, 9);
        let (ws, _) = alu::add(&wa, &wb, 9);
        let lhs = alu::neg(&ws);
        let (rhs, _) = alu::add(&alu::neg(&wa), &alu::neg(&wb), 9);
        assert_eq!(lhs, rhs, "−(a+b) = −a−b (dígitos, n=9)");
    }
}

// ---------------- §6.5 comparação: domínio e antisimetria ----------------

#[test]
fn f4a_cmp_domain_closure_antisymmetry_exhaustive_n2() {
    for a in -4i64..=4 {
        let wa = alu::from_i64(a, 2);
        assert!(alu::valid_digits(&wa));
        for b in -4i64..=4 {
            let wb = alu::from_i64(b, 2);
            let c = alu::cmp(&wa, &wb);
            assert_eq!(c, alu::cmp(&wb, &wa).wrapping_neg(), "antisimetria {a},{b}");
            let expected = (a - b).signum() as i8;
            assert_eq!(c, expected, "ordem numérica {a},{b}");
        }
        assert_eq!(alu::cmp(&wa, &wa), 0);
    }
}

// ---------------- §8 Verificação cruzada Rust ↔ Haskell ----------------

/// Artefato INDEPENDENTE gerado pelo Haskell (experiments/phase4a/
/// gen_haskell_words.hs, com toBT/btValue de BT.hs — sem código Rust envolvido).
/// Codificação canônica de v ∈ [−364, 364] em 6 trits (big-endian '+'/'0'/'-').
const HASKELL_WORDS6: &str = include_str!("../../../results/phase4a/haskell_canonical_width6.txt");

fn parse_haskell_line(line: &str) -> (i64, Vec<i8>) {
    let (v_str, digits_be) = line.split_once('|').expect("formato v|dígitos");
    let v: i64 = v_str.parse().expect("v parseável");
    assert_eq!(digits_be.len(), 6, "largura 6");
    let digits: Vec<i8> = digits_be
        .chars()
        .map(|c| match c {
            '+' => 1,
            '0' => 0,
            '-' => -1,
            _ => panic!("dígito inválido {c}"),
        })
        .collect();
    (v, digits) // big-endian → little-endian: invertido abaixo
}

/// (i) Toda linha do artefato: dígitos válidos, bval = v, e o codificador
///     Rust from_i64(v,6) produz EXATAMENTE os mesmos dígitos.
/// (ii) width_extend preserva os dígitos: para |v| ≤ 13, a palavra canônica
///     em 3 trits estendida a 6 IGUALA o artefato Haskell.
/// (iii) cmp ordena 729² pares exatamente como a ordem numérica v.
#[test]
fn f4a_haskell_crosscheck_width6() {
    let mut words: Vec<(i64, Vec<i8>)> = HASKELL_WORDS6
        .lines()
        .map(|l| {
            let (v, mut d) = parse_haskell_line(l);
            d.reverse(); // big-endian → little-endian
            (v, d)
        })
        .collect();
    assert_eq!(words.len(), 729, "2·364+1 valores");
    assert_eq!(words.first().unwrap().0, -364);
    assert_eq!(words.last().unwrap().0, 364);

    // (i) valor + codificação independente
    for (v, d) in &words {
        assert!(alu::valid_digits(d), "domínio {v}");
        assert_eq!(pack3::bval(d), *v, "bval Haskell {v}");
        assert_eq!(&alu::from_i64(*v, 6), d, "from_i64 vs Haskell {v}");
        assert!(d[5] != 0 || *v == 0 || v.unsigned_abs() < 122, "faixa n=6");
    }

    // (ii) width_extend 3→6 bate com o Haskell (incl. zero, positivos, negativos)
    for (v, d) in &words {
        if v.abs() <= alu::m_max(3) {
            let w3 = alu::from_i64(*v, 3);
            let ext = alu::width_extend(&w3, 3, 6);
            assert_eq!(&ext, d, "extensão 3→6 vs Haskell {v}");
        }
    }

    // (iii) ordem: cmp concorda com a ordem numérica em TODOS os pares
    for (va, da) in &words {
        for (vb, db) in &words {
            let expected = va.cmp(vb) as i8;
            let expected = if expected < 0 { -1 } else if expected > 0 { 1 } else { 0 };
            assert_eq!(alu::cmp(da, db), expected, "cmp {va} vs {vb}");
        }
    }
}

/// Helper de domínio rejeita dígitos fora de {−1,0,+1}.
#[test]
fn f4a_valid_digits_rejects_out_of_domain() {
    assert!(alu::valid_digits(&[-1, 0, 1]));
    assert!(!alu::valid_digits(&[-2, 0, 1]));
    assert!(!alu::valid_digits(&[0, 2]));
    assert!(alu::valid_digits(&[]));
}
