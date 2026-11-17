//! TENERURA — codificação ternária densa: empacotamento em base 3 (§2.2).
//!
//! n trits → Σ uᵢ·3ⁱ com dígitos NÃO-BALANCEADOS uᵢ ∈ {0,1,2} (u = d+1),
//! little-endian (dígito 0 = posição 0). Contêineres por largura:
//!
//! | n  | 3ⁿ−1        | Contêiner |
//! |----|-------------|-----------|
//! | 5  | 242         | `u8`      |
//! | 9  | 19 682      | `u16`     |
//! | 20 | 3 486 784 400 | `u32`   |
//! | 40 | 12 157 665 459 056 928 800 | `u64` |
//!
//! IMPORTANTE (§2.2 do plano): no empacotamento denso TODO valor < 3ⁿ é uma
//! palavra válida — "estados inválidos" aqui são valores fora do alcance,
//! não padrões reservados. A conversão balanced↔unsigned (±1 no dígito) é
//! explícita e testada; não há bits livres para metadados.

use crate::trit::{s3_of_val, ArithTrit};

/// Valor fora do alcance do contêiner/word width.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct OutOfRange;

/// Número de dígitos necessários dado o contêiner (máximos por largura).
pub const MAX_N_U8: usize = 5;
pub const MAX_N_U16: usize = 9;
pub const MAX_N_U32: usize = 20;
pub const MAX_N_U64: usize = 40;

/// 3ⁿ como u128 (para checagens de alcance sem overflow).
#[inline]
pub fn pow3(n: usize) -> u128 {
    (3u128).checked_pow(n as u32).expect("pow3 além de u128")
}

/// Empacota dígitos balanceados (−1,0,+1) como valor denso NÃO-balanceado
/// (u = d+1 por dígito) em `u64`. Erro se n > 40 ou dígito fora do domínio.
pub fn pack3_u64(digits: &[i8]) -> Result<u64, OutOfRange> {
    if digits.len() > MAX_N_U64 {
        return Err(OutOfRange);
    }
    let mut v: u64 = 0;
    let mut w: u64 = 1;
    for &d in digits {
        let u = d as i32 + 1;
        if !(0..=2).contains(&u) {
            return Err(OutOfRange); // dígito inválido ≠ estado reservado
        }
        v += (u as u64) * w;
        w = w.checked_mul(3).ok_or(OutOfRange)?;
    }
    Ok(v)
}

/// Desempacota um valor denso `u64` em `n` dígitos balanceados.
/// Erro se `v >= 3ⁿ` (fora do alcance) — TODA palavra dentro do alcance é válida.
pub fn unpack3_u64(v: u64, n: usize) -> Result<Vec<ArithTrit>, OutOfRange> {
    if n > MAX_N_U64 || (v as u128) >= pow3(n) {
        return Err(OutOfRange);
    }
    let mut x = v;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        let u = x % 3;
        x /= 3;
        out.push(ArithTrit(s3_of_val(u as i8 - 1)));
    }
    Ok(out)
}

/// Versões tipadas por contêiner, com checagem de alcance em tempo de execução.
/// Entrada MAIOR que a capacidade do contêiner é ERRO (rejeição explícita,
/// nunca truncamento silencioso — bug detectado por teste em 2026-11-16).
pub fn pack3_u8(digits: &[i8]) -> Result<u8, OutOfRange> {
    if digits.len() > MAX_N_U8 {
        return Err(OutOfRange);
    }
    let v = pack3_u64(digits)?;
    u8::try_from(v).map_err(|_| OutOfRange)
}

pub fn pack3_u16(digits: &[i8]) -> Result<u16, OutOfRange> {
    if digits.len() > MAX_N_U16 {
        return Err(OutOfRange);
    }
    let v = pack3_u64(digits)?;
    u16::try_from(v).map_err(|_| OutOfRange)
}

pub fn pack3_u32(digits: &[i8]) -> Result<u32, OutOfRange> {
    if digits.len() > MAX_N_U32 {
        return Err(OutOfRange);
    }
    let v = pack3_u64(digits)?;
    u32::try_from(v).map_err(|_| OutOfRange)
}

/// Valor ARITMÉTICO balanceado da palavra: Σ dᵢ·3ⁱ (terceira implementação
/// independente — espelha `btValue` em Haskell e `bval` em Lean, para
/// verificação cruzada tripla). Esquema de Horner: o intermediário nunca
/// excede o valor final em magnitude (correção de overflow em 2026-11-16).
pub fn bval(digits: &[i8]) -> i64 {
    let mut v: i64 = 0;
    for &d in digits.iter().rev() {
        v = v.checked_mul(3)
            .and_then(|x| x.checked_add(d as i64))
            .expect("bval além de i64");
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boundaries_match_the_table() {
        // Tabela do §2.2: últimos valores válidos por contêiner
        assert_eq!(pack3_u8(&[1; 5]).unwrap(), 242);      // 3⁵−1
        assert!(pack3_u8(&[1; 6]).is_err());
        assert_eq!(pack3_u16(&[1; 9]).unwrap(), 19_682);  // 3⁹−1
        assert_eq!(pack3_u32(&[1; 20]).unwrap(), 3_486_784_400); // 3²⁰−1
        assert_eq!(pack3_u64(&[1; 40]).unwrap(), 12_157_665_459_056_928_800); // 3⁴⁰−1
    }

    #[test]
    fn out_of_range_detected() {
        // n=20 não cabe em u32 acima de 3²⁰−1? cabe: 3²⁰−1 < u32::MAX.
        // O que NÃO cabe é valor ≥ 3ⁿ no desempacotamento:
        assert!(unpack3_u64(531_441, 12).is_err()); // 3¹² excede palavra de 12 trits
        assert!(unpack3_u64(81, 4).is_err());       // 3⁴ excede 4 trits
        assert!(unpack3_u64(80, 4).is_ok());
    }

    #[test]
    fn balanced_value_crosscheck_with_haskell() {
        // Haskell: btValue "+-" (big-endian) = bval little-endian [1, -1] = 1 + 3·(−1) = −2
        assert_eq!(bval(&[1, -1]), -2);
        // Lean: bval [p, m] = −2 (mesma palavra, terceira implementação)
        assert_eq!(bval(&[1, -1, 1]), -2 + 9);
    }
}
