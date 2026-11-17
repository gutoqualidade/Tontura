//! TENERURA — testes de representação: exaustivos para domínios pequenos,
//! propriedades com PRNG de semente fixa para domínios grandes (§6 do plano).
//!
//! Sementes registradas: XORSHIFT_SEED = 0x5EED_2026_1116 (data da rodada).
//! PRNG próprio (xorshift64*) para não introduzir dependências externas.

use tenerura_trits::pack2;
use tenerura_trits::pack3;
use tenerura_trits::trit::{s3_of_val, ArithTrit};

/// PRNG xorshift64* — determinístico, semente registrada acima.
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

    /// Índice aleatório de dígito em {−1,0,+1} (3 valores, sem viés grosseiro:
    /// rejeição simples — (u64::MAX % 3) ≈ 6.1e18 rejeições, viés desprezível
    /// para testes, anotado como limitação).
    fn digit(&mut self) -> i8 {
        loop {
            let v = self.next() % 3;
            match v {
                0 => return -1,
                1 => return 0,
                _ => return 1,
            }
        }
    }
}

const SEED: u64 = 0x5EED_2026_1116;

fn digits_rand(n: usize, rng: &mut XorShift) -> Vec<i8> {
    (0..n).map(|_| rng.digit()).collect()
}

// ---------------------------------------------------------------
// Exaustivo: pack2 (cada um dos 256 bytes)
// ---------------------------------------------------------------
#[test]
fn pack2_exhaustive_byte_roundtrip() {
    for byte in 0u16..=255 {
        let b = byte as u8;
        let buf = [b];
        // Cada trit é decidido INDEPENDENTEMENTE pelo seu próprio nibble
        let codes: Vec<u8> = (0..4).map(|i| (b >> (2 * i)) & 0b11).collect();
        for (i, &code) in codes.iter().enumerate() {
            match pack2::get_trit(&buf, i) {
                Ok(t) => {
                    assert_ne!(code, 0b11, "trit {i} Ok mas código reservado: {b:#010b}");
                    assert_eq!(pack2::code_of(t), code, "decode inconsistente");
                }
                Err(e) => {
                    assert_eq!(code, 0b11, "trit {i} Err mas código válido: {b:#010b}");
                    assert_eq!(e.0, 0b11);
                }
            }
        }
        // Roundtrip completo apenas para bytes totalmente válidos
        if codes.iter().all(|&c| c != 0b11) {
            let mut back = vec![0u8; 1];
            for i in 0..4 {
                pack2::set_trit(&mut back, i, pack2::get_trit(&buf, i).unwrap());
            }
            assert_eq!(back[0], b, "roundtrip falhou para {b:#010b}");
        }
    }
}

// ---------------------------------------------------------------
// Exaustivo: pack3 em u8 (n=5: 243 valores) e u16 (n=9: 19 683 valores)
// ---------------------------------------------------------------
#[test]
fn pack3_exhaustive_u8_n5() {
    for v in 0u32..243 {
        let digits = pack3::unpack3_u64(v as u64, 5).unwrap().iter()
            .map(|t| t.val()).collect::<Vec<_>>();
        assert_eq!(pack3::pack3_u8(&digits).unwrap() as u32, v, "roundtrip v={v}");
    }
    // Fora do alcance: 243..=255 em 5 trits é inválido
    for v in 243u32..=255 {
        assert!(pack3::unpack3_u64(v as u64, 5).is_err(), "v={v} deveria ser inválido");
    }
}

#[test]
fn pack3_exhaustive_u16_n9() {
    for v in 0u32..19_683 {
        let digits = pack3::unpack3_u64(v as u64, 9).unwrap().iter()
            .map(|t| t.val()).collect::<Vec<_>>();
        assert_eq!(pack3::pack3_u16(&digits).unwrap() as u32, v, "roundtrip v={v}");
    }
}

// ---------------------------------------------------------------
// Propriedades com PRNG (domínios grandes: u32 n=20, u64 n=27/40)
// ---------------------------------------------------------------
#[test]
fn pack3_property_roundtrip_u32_n20() {
    let mut rng = XorShift(SEED);
    for _ in 0..100_000 {
        let digits = digits_rand(20, &mut rng);
        let v = pack3::pack3_u32(&digits).unwrap();
        assert!(v < pack3::pow3(20) as u32, "valor denso fora do alcance");
        let back = pack3::unpack3_u64(v as u64, 20).unwrap();
        assert_eq!(back.iter().map(|t| t.val()).collect::<Vec<_>>(), digits);
    }
}

#[test]
fn pack3_property_roundtrip_u64_n27_and_n40() {
    let mut rng = XorShift(SEED ^ 0xA5A5);
    for _ in 0..50_000 {
        for &n in &[27usize, 40] {
            let digits = digits_rand(n, &mut rng);
            let v = pack3::pack3_u64(&digits).unwrap();
            let back = pack3::unpack3_u64(v, n).unwrap();
            assert_eq!(back.iter().map(|t| t.val()).collect::<Vec<_>>(), digits);
        }
    }
}

#[test]
fn pack2_property_word27() {
    let mut rng = XorShift(SEED ^ 0xC0DE);
    for _ in 0..50_000 {
        let ds = digits_rand(27, &mut rng);
        let word: Vec<ArithTrit> = ds.iter().map(|&d| ArithTrit(s3_of_val(d))).collect();
        let v = pack2::pack_word(&word);
        assert_eq!(v >> 54, 0, "bits 54..64 devem estar zerados");
        let back = pack2::unpack_word(v, 27).unwrap();
        assert_eq!(back.iter().map(|t| t.val()).collect::<Vec<_>>(), ds);
    }
}

// ---------------------------------------------------------------
// Diferencial: pack2 e pack3 transportam os MESMOS trits (§6)
// ---------------------------------------------------------------
#[test]
fn differential_pack2_vs_pack3() {
    let mut rng = XorShift(SEED ^ 0xD1FF);
    for _ in 0..20_000 {
        let ds = digits_rand(27, &mut rng);
        let word: Vec<ArithTrit> = ds.iter().map(|&d| ArithTrit(s3_of_val(d))).collect();
        let p2 = pack2::unpack_word(pack2::pack_word(&word), 27).unwrap();
        let p3 = pack3::unpack3_u64(pack3::pack3_u64(&ds).unwrap(), 27).unwrap();
        assert_eq!(
            p2.iter().map(|t| t.val()).collect::<Vec<_>>(),
            p3.iter().map(|t| t.val()).collect::<Vec<_>>()
        );
    }
}

// ---------------------------------------------------------------
// Valor aritmético: valores fechados (terceira implementação de bval)
// ---------------------------------------------------------------
#[test]
fn bval_closed_form_values() {
    // Todas +1 com n trits: (3ⁿ−1)/2
    assert_eq!(pack3::bval(&[1; 27]), 3_812_798_742_493);          // (3²⁷−1)/2
    // Todas −1: −(3ⁿ−1)/2
    assert_eq!(pack3::bval(&[-1; 40]), -6_078_832_729_528_464_400); // −(3⁴⁰−1)/2
    // Coerência com a tabela: Σ 1·3ⁱ para n=5 = 121 = (3⁵−1)/2
    assert_eq!(pack3::bval(&[1; 5]), 121);
}
