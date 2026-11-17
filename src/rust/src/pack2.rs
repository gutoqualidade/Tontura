//! TENERURA — codificação binária fixa: 2 bits por trit (§2.1 do plano).
//!
//! | Código | Trit     |
//! |--------|----------|
//! | `00`   | 0        |
//! | `01`   | +1       |
//! | `10`   | −1       |
//! | `11`   | Reservado (estado inválido / integridade) |
//!
//! Densidade: 4 trits por byte. Indexação por trit é O(1):
//! byte = idx / 4, nibble = idx % 4 (little-endian: trit 0 é o nibble baixo).

use crate::trit::{s3_of_val, ArithTrit};

/// Código do estado reservado (inválido).
pub const CODE_INVALID: u8 = 0b11;

/// Estado inválido detectado (código reservado 0b11 em qualquer nibble).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct InvalidCode(pub u8);

/// Código de 2 bits de um trit aritmético.
#[inline]
pub fn code_of(t: ArithTrit) -> u8 {
    match t.val() {
        -1 => 0b10,
        0 => 0b00,
        _ => 0b01,
    }
}

/// Trit a partir do código. `Err` para o código reservado 0b11.
#[inline]
pub fn trit_of_code(c: u8) -> Result<ArithTrit, InvalidCode> {
    match c {
        0b00 => Ok(ArithTrit(s3_of_val(0))),
        0b01 => Ok(ArithTrit(s3_of_val(1))),
        0b10 => Ok(ArithTrit(s3_of_val(-1))),
        _ => Err(InvalidCode(c)),
    }
}

/// Lê o trit `idx` de um buffer compactado 2-bits (4 trits/byte).
/// Índices fora do alcance geram pânico do slice — o chamador controla o alcance.
#[inline]
pub fn get_trit(buf: &[u8], idx: usize) -> Result<ArithTrit, InvalidCode> {
    let byte = buf[idx / 4];
    let shift = 2 * (idx % 4) as u32;
    trit_of_code((byte >> shift) & 0b11)
}

/// Escreve o trit `idx` no buffer compactado 2-bits (read-modify-write).
#[inline]
pub fn set_trit(buf: &mut [u8], idx: usize, t: ArithTrit) {
    let byte = &mut buf[idx / 4];
    let shift = 2 * (idx % 4) as u32;
    *byte = (*byte & !(0b11 << shift)) | (code_of(t) << shift);
}

/// Empacota uma palavra de até 27 trits em um `u64` (54 bits usados, bits
/// 54..64 zerados — o chamador pode validá-los como integridade).
pub fn pack_word(word: &[ArithTrit]) -> u64 {
    let mut v: u64 = 0;
    for (i, t) in word.iter().enumerate() {
        v |= (code_of(*t) as u64) << (2 * i);
    }
    v
}

/// Desempacota uma palavra `u64` em `n` trits; detecta códigos reservados.
pub fn unpack_word(v: u64, n: usize) -> Result<Vec<ArithTrit>, InvalidCode> {
    assert!(n <= 27, "palavra 2-bits cabe em u64 até 27 trits");
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        out.push(trit_of_code(((v >> (2 * i)) & 0b11) as u8)?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trit::S3;

    fn trits(vals: &[i8]) -> Vec<ArithTrit> {
        vals.iter().map(|&v| ArithTrit(s3_of_val(v))).collect()
    }

    #[test]
    fn code_table() {
        assert_eq!(code_of(ArithTrit(S3::Z)), 0b00);
        assert_eq!(code_of(ArithTrit(S3::P)), 0b01);
        assert_eq!(code_of(ArithTrit(S3::M)), 0b10);
        assert!(trit_of_code(0b11).is_err()); // reservado detectado
    }

    #[test]
    fn buffer_roundtrip_and_reserved_detection() {
        // 8 trits, 2 bytes
        let mut buf = vec![0u8; 2];
        for (i, v) in [1i8, 0, -1, 1, -1, 0, 0, 1].iter().enumerate() {
            set_trit(&mut buf, i, ArithTrit(s3_of_val(*v)));
        }
        for (i, v) in [1i8, 0, -1, 1, -1, 0, 0, 1].iter().enumerate() {
            assert_eq!(get_trit(&buf, i).unwrap().val(), *v);
        }
        // Injetar código reservado: detectável
        buf[0] |= 0b11 << 6; // trit 3 = reservado
        assert!(get_trit(&buf, 3).is_err());
        assert!(get_trit(&buf, 7).is_ok()); // outros intactos
    }

    #[test]
    fn word27_roundtrip() {
        let w = trits(&[-1; 27]);
        let v = pack_word(&w);
        let expect: u64 = (0..27).fold(0u64, |acc, i| acc | (0b10u64 << (2 * i)));
        assert_eq!(v, expect);
        let back = unpack_word(v, 27).unwrap();
        assert_eq!(back.iter().map(|t| t.val()).collect::<Vec<_>>(), vec![-1; 27]);
        // Bits 54..64 devem estar zerados (integridade do contêiner)
        assert_eq!(v >> 54, 0);
    }

    #[test]
    fn word_reserved_detected() {
        // Injeta 0b11 no trit 5 de uma palavra
        let v: u64 = 0b11 << 10;
        assert!(unpack_word(v, 6).is_err());
    }
}
