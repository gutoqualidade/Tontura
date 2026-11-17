//! TENERURA T27 — teste DIFERENCIAL da palavra do emulador contra a ALU
//! auditada da Fase 4A (`tenerura_trits::alu`, dígitos `Vec<i8>`).
//!
//! Objetivo: a representação rápida (`i64`) do emulador não pode divergir
//! dos contratos já provados/testados (docs/05-alu-especificacao.md).
//! Método: valores de fronteira × fronteira (exaustivo no conjunto) +
//! amostras pseudoaleatórias; PRNG xorshift64*, semente registrada.
//! Limitação: n = 27 não admite exaustão (3²⁷ palavras) — a cobertura é
//! fronteiras + amostragem, não prova.

use tenerura_t27::word::{half, Word, M27, P27, POW3, TRITS};
use tenerura_trits::alu;
use tenerura_trits::trit::{s3_of_val, KleeneTrit};

const SEED: u64 = 0x5EED_2026_1116 ^ 0x7027;

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
    fn word(&mut self) -> Word {
        // metade das amostras em toda a janela, metade com magnitude pequena
        // aleatória (exercita carries curtos e zeros altos)
        let r = self.next();
        let w = Word::wrap128(r as i128);
        if self.next() % 2 == 0 {
            w
        } else {
            w.shift(-((self.next() % 27) as i64))
        }
    }
}

fn boundary() -> Vec<Word> {
    let mut v = vec![0, 1, -1, 2, -2, 3, -3, 4, -4, 13, -13, 364, -364, 365, -365];
    for k in 1..TRITS {
        for d in [-1, 0, 1] {
            v.push(POW3[k] + d);
            v.push(-POW3[k] + d);
            v.push(half(k) + d);
            v.push(-half(k) + d);
        }
    }
    v.extend([M27, -M27, M27 - 1, -M27 + 1, M27 / 2, -(M27 / 2), M27 / 3]);
    v.sort_unstable();
    v.dedup();
    v.into_iter().map(Word::lit).collect()
}

fn digits(w: Word) -> Vec<i8> {
    w.trits().to_vec()
}

fn check_pair(a: Word, b: Word) {
    let (da, db) = (digits(a), digits(b));
    // codificação: dois codificadores independentes concordam
    assert_eq!(da, alu::from_i64(a.val(), TRITS), "dígitos de {a}");
    assert_eq!(alu::bval(&da), a.val());

    let (s, c) = a.add(b);
    let (ds, ovf) = alu::add(&da, &db, TRITS);
    assert_eq!(digits(s), ds, "add {a} {b}");
    assert_eq!(c != 0, ovf, "overflow add {a} {b}");
    assert_eq!(a.val() + b.val(), s.val() + c as i64 * P27, "identidade do carry {a} {b}");

    let (d, c) = a.sub(b);
    let (dd, ovf) = alu::sub(&da, &db, TRITS);
    assert_eq!(digits(d), dd, "sub {a} {b}");
    assert_eq!(c != 0, ovf, "overflow sub {a} {b}");

    let (lo, hi) = a.mul(b);
    let (dm, ovf) = alu::mul(&da, &db, TRITS);
    assert_eq!(digits(lo), dm, "mul {a} {b}");
    assert_eq!(hi.val() != 0, ovf, "overflow mul {a} {b}");
    assert_eq!(
        a.val() as i128 * b.val() as i128,
        lo.val() as i128 + hi.val() as i128 * P27 as i128,
        "mul hi/lo {a} {b}"
    );

    assert_eq!(a.cmp3(b), alu::cmp(&da, &db), "cmp {a} {b}");
    assert_eq!(digits(a.neg()), alu::neg(&da), "neg {a}");

    // Kleene trit a trit contra os tipos semânticos da Fase 2
    let (mn, mx) = (digits(a.kmin(b)), digits(a.kmax(b)));
    for i in 0..TRITS {
        let (x, y) = (KleeneTrit(s3_of_val(da[i])), KleeneTrit(s3_of_val(db[i])));
        assert_eq!(mn[i], x.and(y).0.val(), "kmin trit {i} de {a} {b}");
        assert_eq!(mx[i], x.or(y).0.val(), "kmax trit {i} de {a} {b}");
    }

    // divisão centrada (semântica nova): identidade, limite do resto, simetrias
    let (q, r) = a.divrem(b);
    assert_eq!(a.val(), q.val() * b.val() + r.val(), "a = q·b + r  {a} {b}");
    if b.val() != 0 {
        assert!(2 * r.val().abs() <= b.val().abs(), "|2r| ≤ |b|  {a} {b}");
        let (q2, r2) = a.neg().divrem(b);
        assert_eq!((q2, r2), (q.neg(), r.neg()), "simetria de negação do dividendo {a} {b}");
        let (q3, r3) = a.divrem(b.neg());
        assert_eq!((q3, r3), (q.neg(), r), "simetria de negação do divisor {a} {b}");
    } else {
        assert_eq!((q.val(), r), (0, a));
    }
}

fn check_shifts(a: Word) {
    let da = digits(a);
    let (dl, _ovf) = alu::shl(&da);
    assert_eq!(digits(a.shift(1)), dl, "shl {a}");
    assert_eq!(digits(a.shift(-1)), alu::shr(&da), "shr {a}");
    let mut up = da.clone();
    for k in 0..=TRITS {
        assert_eq!(digits(a.shift(-(k as i64))), alu::trunc_low(&da, k), "trunc_low {a} k={k}");
        assert_eq!(digits(a.shift(k as i64)), up, "shl^{k} {a}");
        up = alu::shl(&up).0;
        // DIV por 3ᵏ coincide com o deslocamento (sem a verruga sar≠idiv do binário)
        if k < TRITS {
            assert_eq!(a.divrem(Word::lit(POW3[k])).0, a.shift(-(k as i64)), "div 3^{k} = shift {a}");
        }
    }
    assert_eq!(a.shift(40).val(), 0);
    assert_eq!(a.shift(-40).val(), 0);
}

#[test]
fn boundary_cross_product_matches_audited_alu() {
    let b = boundary();
    for &x in &b {
        check_shifts(x);
        for &y in &b {
            check_pair(x, y);
        }
    }
}

#[test]
fn random_pairs_match_audited_alu() {
    let mut rng = XorShift(SEED);
    for _ in 0..40_000 {
        let (a, b) = (rng.word(), rng.word());
        check_pair(a, b);
    }
    for _ in 0..4_000 {
        check_shifts(rng.word());
    }
}

#[test]
fn field_extraction_matches_digit_slices() {
    let mut rng = XorShift(SEED ^ 0xF1E1D);
    for _ in 0..20_000 {
        let w = rng.word();
        let d = digits(w);
        let lo = (rng.next() % 27) as usize;
        let len = 1 + (rng.next() as usize % (27 - lo));
        assert_eq!(w.field(lo, len), alu::bval(&d[lo..lo + len]), "campo [{lo},{}) de {w}", lo + len);
    }
}
