//! TENERURA Fase 4A — ALU ternária auditada (especificação: docs/05-alu-especificacao.md).
//!
//! PRINCÍPIO: a ALU é ternária na SEMÂNTICA. Ela opera sobre dígitos
//! balanceados little-endian `Vec<i8>` ∈ {−1,0,+1} — representação-agnóstica
//! (pack2/pack3 apenas transportam; nada aqui depende deles).
//!
//! DOMÍNIO: toda palavra com dígitos ∈ {−1,0,+1} de largura n está na janela
//! [−Mₙ, Mₙ] (|Σdᵢ3ⁱ| ≤ Σ3ⁱ = Mₙ) e é o representante ÚNICO do seu valor
//! (injetividade da codificação — provado em Lean, Phase4A.encoding_inj).
//! Não existem palavras não canônicas no domínio.
//!
//! Contratos (auditados, Fase 4A):
//! - Faixa: Mₙ = (3ⁿ−1)/2; janela única por congruência mod 3ⁿ (3ⁿ ímpar).
//! - add: schoolbook, carry ∈ {−1,0,1}; overflow ⟺ |soma exata| > Mₙ ⟺
//!   carry final ≠ 0; palavra entregue = wrap balanceado (≡ soma mod 3ⁿ).
//! - shl: ×3; overflow ⟺ trit de topo ≠ 0 (⟺ |3V| > Mₙ — provado em Lean).
//! - shr (= trunc_low k=1): descarta o LSB; resultado = inteiro mais próximo
//!   de V/3, SEM EMPATES (|LSB| ≤ 1 < 3/2). NÃO é inverso de shl: shl∘shr
//!   zera o LSB = V − d₀ (o dígito descartado É o resto — há perda de
//!   informação). Reciprocamente, shr∘shl = identidade ⟺ shl NÃO estourou
//!   (topo = 0): shl descarta o trit de topo, que shr não pode recuperar.
//! - trunc_low k: descarta k trits baixos = arredondar ao múltiplo de 3ᵏ
//!   mais próximo, sem empates (espelha btTrunc do Haskell).
//! - mul: produto exato em i128 (n ≤ 40 ⇒ |v| ≤ 6,08e18, produto ≤ 3,70e37
//!   < i128::MAX ≈ 1,70e38), truncado a n trits; overflow ⟺ |produto| > Mₙ.
//! - cmp: lexicográfica do topo — correta para TODO o domínio (no maior dígito
//!   diferente, |resto| ≤ 3ᵏ−1 < 3ᵏ não sustenta o dígito de cima).
//! - width_extend (antes `sign_extend` — nome aposentado, ver doc): preenche
//!   o lado ALTO com ZERO; preserva valor e canonicidade.
//! - ISA NÃO antecipada: sem registradores/memória aqui.

/// Limite da faixa balanceada de n trits: (3ⁿ−1)/2. n ≤ 40 cabe em i64
/// (M₄₀ = 6 078 832 729 528 464 400 < i64::MAX), mas 3⁴⁰ NÃO cabe —
/// a recorrência Mₖ₊₁ = 3·Mₖ + 1 evita o overflow intermediário.
#[inline]
pub fn m_max(n: usize) -> i64 {
    assert!(n <= 40, "faixa além de i64");
    let mut m: i64 = 0;
    for _ in 0..n {
        m = m * 3 + 1;
    }
    m
}

/// Valor aritmético Σ dᵢ·3ⁱ (delega a pack3::bval — 3ª via verificada).
pub fn bval(digits: &[i8]) -> i64 {
    crate::pack3::bval(digits)
}

/// Converte v em palavra canônica de n trits: v mod 3ⁿ em dígitos balanceados
/// (janela [−(3ⁿ−1)/2, (3ⁿ−1)/2], única representante — 3ⁿ ímpar).
/// 3ⁿ não cabe em i64 para n = 40 → delega a i128.
pub fn from_i64(v: i64, n: usize) -> Vec<i8> {
    from_i128(v as i128, n)
}

/// Negação digitwise: −(Σdᵢ3ⁱ) = Σ(−dᵢ)3ⁱ. Sem carry, preserva a faixa.
pub fn neg(a: &[i8]) -> Vec<i8> {
    a.iter().map(|d| -d).collect()
}

/// Soma schoolbook de duas palavras de n trits. Retorna (palavra, overflow).
/// Overflow ⟺ carry final ≠ 0; a palavra entregue é o wrap balanceado.
pub fn add(a: &[i8], b: &[i8], n: usize) -> (Vec<i8>, bool) {
    assert_eq!(a.len(), n, "add: largura de a");
    assert_eq!(b.len(), n, "add: largura de b");
    let mut c: i8 = 0;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let s = a[i] + b[i] + c; // ∈ [−3, 3]
        let (d, nc) = match s {
            -3..=-2 => (s + 3, -1),
            -1..=1 => (s, 0),
            _ => (s - 3, 1), // 2..=3
        };
        out.push(d);
        c = nc;
    }
    (out, c != 0)
}

/// Subtração: a + (−b). Negação balanceada não tem custo oculto AQUI porque
/// é digitwise; o overflow da subtração é o da soma com −b (registrado).
pub fn sub(a: &[i8], b: &[i8], n: usize) -> (Vec<i8>, bool) {
    add(a, &neg(b), n)
}

/// Multiplicação: produto exato em i128, truncado a n trits.
/// Retorna (palavra truncada, overflow ⟺ |produto| > (3ⁿ−1)/2).
pub fn mul(a: &[i8], b: &[i8], n: usize) -> (Vec<i8>, bool) {
    assert!(n <= 40, "mul: produto exato garantido apenas até n=40");
    let va = bval(a) as i128;
    let vb = bval(b) as i128;
    let v = va * vb;
    let m = m_max(n) as i128;
    let overflow = v.abs() > m;
    (from_i128(v, n), overflow)
}

/// Balanced mod 3ⁿ para i128 (usado por mul; n ≤ 40).
fn from_i128(v: i128, n: usize) -> Vec<i8> {
    let mut p: i128 = 1;
    for _ in 0..n {
        p *= 3;
    }
    let mut x = v.rem_euclid(p);
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        let u = x % 3;
        x /= 3;
        out.push(match u {
            0 => 0,
            1 => 1,
            _ => {
                x += 1;
                -1
            }
        });
    }
    out
}

/// Comparação ternária: −1 / 0 / +1 (sinal de a−b), lexicográfica do topo.
/// Produz um TRIT, não um desvio — conversão para controle é decisão de ISA.
/// Correta para TODO o domínio {−1,0,+1}ⁿ: no par de dígitos de topo onde
/// diferem, |diferença das caudas| ≤ Σ3ⁱ < 3ᵏ ≤ diferença dos dígitos de topo
/// (injetividade da codificação — provado em Lean, Phase4A.encoding_inj).
pub fn cmp(a: &[i8], b: &[i8]) -> i8 {
    assert_eq!(a.len(), b.len(), "cmp: larguras iguais (usar width_extend)");
    for i in (0..a.len()).rev() {
        if a[i] != b[i] {
            return if a[i] > b[i] { 1 } else { -1 };
        }
    }
    0
}

/// Deslocamento ternário à esquerda (×3): dígitos sobem, LSB ← 0.
/// Overflow ⟺ trit de topo de entrada ≠ 0. Retorna (palavra, overflow).
pub fn shl(a: &[i8]) -> (Vec<i8>, bool) {
    let n = a.len();
    let overflow = a[n - 1] != 0;
    let mut out = Vec::with_capacity(n);
    out.push(0);
    out.extend_from_slice(&a[..n - 1]);
    (out, overflow)
}

/// Deslocamento ternário à direita (÷3): queda do LSB com arredondamento ao
/// mais próximo (A6 — sem empates). Os n−1 dígitos restantes JÁ são o valor
/// exato; repõe-se a largura com zero (veja `width_extend`).
/// NÃO é inverso de `shl`: shl∘shr zera o LSB ⇒ bval = V − d₀ (o LSB
/// descartado é o resto — HÁ perda de informação). Reciprocamente,
/// shr∘shl = identidade sobre dígitos ⟺ shl não estourou (topo = 0);
/// shl descarta o trit de topo e shr não o recupera.
pub fn shr(a: &[i8]) -> Vec<i8> {
    let mut out: Vec<i8> = a[1..].to_vec();
    out.push(0); // extensão com ZERO — ver doc de width_extend
    out
}

/// Truncamento de k trits baixos (espelho de `btTrunc` do Haskell):
/// mantém os dígitos das posições k..n, repõe a largura com k zeros.
/// Valor = inteiro mais próximo de V/3ᵏ, SEM EMPATES (3ᵏ é ímpar e o resto
/// representável |V mod 3ᵏ| ≤ (3ᵏ−1)/2 < 3ᵏ/2). Com k=1 é `shr`.
/// Preserva largura e canonicidade (o resultado é palavra de dígitos válidos
/// e |valor| ≤ Mₙ por ser palavra do domínio).
pub fn trunc_low(a: &[i8], k: usize) -> Vec<i8> {
    let n = a.len();
    assert!(k <= n, "trunc_low: k > largura");
    let mut out: Vec<i8> = a[k..].to_vec();
    out.resize(n, 0);
    out
}

/// Extensão de largura de `from` para `to` trits: PREENCHIMENTO COM ZERO.
///
/// RESULTADO NEGATIVO REGISTRADO (2026-11-16): a suposição "extensão de sinal
/// = replicar o trit de topo" (herdada do plano) foi REFUTADA pelos testes.
/// Em binário em complemento-de-2 funciona porque o peso do bit de sinal é
/// −2ⁿ⁻¹ e duplicá-lo vale +2ⁿ⁻¹ ≡ 0 (mod 2ⁿ). No ternário balanceado TODOS
/// os pesos são positivos: replicar t adiciona t·(3ᵗᵒ−3ᶠʳᵒᵐ)/2 ≠ 0 e MUDA o
/// valor (ex.: [−1,−1] = −4 estendido por replicação vira [−1,−1,−1] = −13).
/// A referência Haskell (TernArith.hs) sempre preencheu com Z — a ALU agora
/// coincide com ela. Preenchimento com zero preserva valor E canonicidade:
/// w canônico em n trits continua sendo o único representante de v módulo
/// 3ᵗᵒ com |v| ≤ (3ᶠʳᵒᵐ−1)/2 < (3ᵗᵒ−1)/2.
pub fn width_extend(a: &[i8], from: usize, to: usize) -> Vec<i8> {
    assert!(from <= to && a.len() == from);
    let mut out = a.to_vec();
    out.resize(to, 0);
    out
}

/// Domínio: a palavra tem TODOS os dígitos em {−1,0,+1}?
/// Helper de auditoria — pré-condição de todas as operações acima.
pub fn valid_digits(a: &[i8]) -> bool {
    a.iter().all(|&d| d >= -1 && d <= 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(v: i64, n: usize) -> Vec<i8> {
        from_i64(v, n)
    }

    #[test]
    fn from_i64_canonical_range_and_value() {
        // n=3: faixa −13..13, 27 valores canônicos
        for v in -13..=13 {
            let w = word(v, 3);
            assert_eq!(bval(&w), v);
        }
        // wrap: 14 → −13 (mod 27 balanceado)
        assert_eq!(bval(&word(14, 3)), 14 - 27);
        assert_eq!(bval(&word(-14, 3)), -14 + 27);
    }

    #[test]
    fn add_exhaustive_n2_no_overflow() {
        // pares dentro da faixa n=2 (−4..4): 81 combinações
        for a in -4..=4 {
            for b in -4..=4 {
                let (w, ovf) = add(&word(a, 2), &word(b, 2), 2);
                if a + b >= -4 && a + b <= 4 {
                    assert!(!ovf, "falso overflow {a}+{b}");
                    assert_eq!(bval(&w), a + b);
                } else {
                    assert!(ovf, "overflow ausente {a}+{b}");
                    // wrap: valor entregue ≡ a+b mod 9 balanceado
                    let wrapped = if a + b > 4 { a + b - 9 } else { a + b + 9 };
                    assert_eq!(bval(&w), wrapped);
                }
            }
        }
    }

    #[test]
    fn mul_exhaustive_n2() {
        for a in -4..=4 {
            for b in -4..=4 {
                let (w, ovf) = mul(&word(a, 2), &word(b, 2), 2);
                let p = a * b;
                assert_eq!(ovf, p.abs() > 4, "flag mul {a}·{b}");
                // verificação direta: w ≡ p (mod 9) e |bval(w)| ≤ 4
                assert_eq!((bval(&w) - p).rem_euclid(9), 0, "mul {a}·{b}");
                assert!(bval(&w).abs() <= 4);
            }
        }
    }

    #[test]
    fn shr_is_round_to_nearest() {
        // A6: ÷3 com arredondamento; exaustivo n=3
        for v in -13..=13 {
            let w = shr(&word(v, 3));
            // round(v/3) sem empates (3 é ímpar)
            let q = (v as f64 / 3.0).round() as i64;
            let q_exact = {
                // round-half-even NÃO deve ocorrer: v/3 nunca é x.5
                let r = v.rem_euclid(3);
                let base = v.div_euclid(3);
                if r >= 2 {
                    base + 1
                } else {
                    base
                }
            };
            assert_eq!(q, q_exact, "sem empate v/3: {v}");
            assert_eq!(bval(&w), q_exact, "shr {v}");
        }
    }

    #[test]
    fn shl_overflow_iff_top_nonzero() {
        for v in -4..=4 {
            let w = word(v, 2);
            let (_, ovf) = shl(&w);
            assert_eq!(ovf, w[1] != 0, "shl {v}");
            if !ovf {
                assert_eq!(bval(&shl(&w).0), 3 * v);
            }
        }
    }

    #[test]
    fn sign_extension_value_preserving() {
        // mesma palavra, larguras diferentes, mesmo valor
        for v in -4..=4 {
            let w2 = word(v, 2);
            let w4 = width_extend(&w2, 2, 4);
            assert_eq!(bval(&w4), v);
            // canonicidade: extensão = forma canônica em 4 trits
            assert_eq!(from_i64(v, 4), w4);
        }
    }
}
