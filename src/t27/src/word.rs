//! TENERURA T27 — palavra ternária balanceada de 27 trits (docs/07-isa-t27.md §2).
//!
//! SEMÂNTICA: a palavra é o inteiro V = Σ tᵢ·3ⁱ, tᵢ ∈ {−1,0,+1}, i = 0..26,
//! na janela [−M, M] com M = (3²⁷−1)/2. Os contratos são os da ALU auditada
//! (docs/05-alu-especificacao.md): wrap balanceado, carry ∈ {−1,0,+1},
//! deslocamento à direita = arredondamento ao mais próximo sem empates,
//! extensão de largura por ZERO.
//!
//! REPRESENTAÇÃO (decisão do EMULADOR, não da arquitetura): o valor V num
//! `i64`. A aritmética vira aritmética nativa + redução à janela; operações
//! trit a trit convertem para dígitos sob demanda. Esta escolha foi medida
//! contra alternativas em `bin/bench_word.rs` (results/phase5/). A ALU de
//! dígitos `Vec<i8>` da Fase 4A continua sendo o ORÁCULO: tests/word_diff.rs
//! compara cada operação daqui com ela.

/// Largura da palavra em trits.
pub const TRITS: usize = 27;

/// 3ⁱ para i = 0..=27.
pub const POW3: [i64; 28] = {
    let mut t = [1i64; 28];
    let mut i = 1;
    while i < 28 {
        t[i] = t[i - 1] * 3;
        i += 1;
    }
    t
};

/// 3²⁷ — número de palavras distintas.
pub const P27: i64 = POW3[27];
/// M = (3²⁷−1)/2 — limite da janela balanceada.
pub const M27: i64 = (P27 - 1) / 2;

/// (3ᵏ−1)/2: limite da janela balanceada de k trits (k ≤ 27).
#[inline]
pub const fn half(k: usize) -> i64 {
    (POW3[k] - 1) / 2
}

/// Palavra de 27 trits. Invariante: |valor| ≤ M27.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Word(i64);

impl std::fmt::Debug for Word {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::fmt::Display for Word {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Word {
    pub const ZERO: Word = Word(0);
    pub const ONE: Word = Word(1);
    pub const MAX: Word = Word(M27);
    pub const MIN: Word = Word(-M27);

    /// Constrói a palavra se o valor está na janela; `None` caso contrário.
    #[inline]
    pub const fn new(v: i64) -> Option<Word> {
        if v >= -M27 && v <= M27 {
            Some(Word(v))
        } else {
            None
        }
    }

    /// Constrói a partir de um valor que o chamador garante estar na janela.
    /// Pânico fora dela — uso: constantes e resultados já reduzidos.
    #[inline]
    pub const fn lit(v: i64) -> Word {
        assert!(v >= -M27 && v <= M27, "Word::lit fora da janela");
        Word(v)
    }

    /// Wrap balanceado: único representante de v (mod 3²⁷) na janela.
    #[inline]
    pub fn wrap(v: i64) -> Word {
        if v >= -M27 && v <= M27 {
            return Word(v);
        }
        Word::wrap128(v as i128)
    }

    /// Wrap balanceado para intermediários de 128 bits (produtos).
    #[inline]
    pub fn wrap128(v: i128) -> Word {
        let r = (v + M27 as i128).rem_euclid(P27 as i128) - M27 as i128;
        Word(r as i64)
    }

    #[inline]
    pub const fn val(self) -> i64 {
        self.0
    }

    /// Negação: trit a trit, nunca estoura (janela simétrica).
    #[inline]
    pub fn neg(self) -> Word {
        Word(-self.0)
    }

    /// Soma com carry: (wrap(a+b), c) com a + b = wrap + c·3²⁷, c ∈ {−1,0,+1}.
    #[inline]
    pub fn add(self, o: Word) -> (Word, i8) {
        let s = self.0 + o.0; // |s| ≤ 2M: cabe folgado em i64
        if s > M27 {
            (Word(s - P27), 1)
        } else if s < -M27 {
            (Word(s + P27), -1)
        } else {
            (Word(s), 0)
        }
    }

    /// Subtração = soma com a negação (a negação é exata, logo o carry de
    /// a−b é o carry de a+(−b) — não existe o caso assimétrico do binário).
    #[inline]
    pub fn sub(self, o: Word) -> (Word, i8) {
        self.add(o.neg())
    }

    /// Produto exato de 54 trits como (lo, hi): a·b = lo + hi·3²⁷,
    /// ambos na janela (decomposição balanceada, única).
    #[inline]
    pub fn mul(self, o: Word) -> (Word, Word) {
        let p = self.0 as i128 * o.0 as i128;
        let lo = Word::wrap128(p);
        let hi = (p - lo.0 as i128) / P27 as i128;
        (lo, Word(hi as i64))
    }

    /// Comparação exata: sinal de a−b como trit (sem wrap).
    #[inline]
    pub fn cmp3(self, o: Word) -> i8 {
        (self.0 > o.0) as i8 - (self.0 < o.0) as i8
    }

    /// Sinal da palavra como trit.
    #[inline]
    pub fn sign(self) -> i8 {
        (self.0 > 0) as i8 - (self.0 < 0) as i8
    }

    /// Deslocamento ternário por k posições (UMA operação, sentido pelo sinal):
    /// k > 0: dígitos sobem (×3ᵏ com wrap — os trits de topo saem);
    /// k < 0: descarta |k| trits baixos = inteiro mais próximo de V/3^|k|;
    /// |k| ≥ 27: zero.
    pub fn shift(self, k: i64) -> Word {
        if k == 0 {
            self
        } else if k >= TRITS as i64 || k <= -(TRITS as i64) {
            Word::ZERO
        } else if k > 0 {
            Word::wrap128(self.0 as i128 * POW3[k as usize] as i128)
        } else {
            let k = (-k) as usize;
            Word((self.0 + half(k)).div_euclid(POW3[k]))
        }
    }

    /// Divisão centrada: q = inteiro mais próximo de a/b (empate → o mais
    /// próximo de zero), r = a − q·b, |2r| ≤ |b|. Total: b = 0 ⇒ (0, a),
    /// preservando a = q·b + r. Coincide com `shift` para b = 3ᵏ.
    /// (Semântica NOVA da Fase 5 — docs/07-isa-t27.md §4.3.)
    pub fn divrem(self, o: Word) -> (Word, Word) {
        let (a, b) = (self.0, o.0);
        if b == 0 {
            return (Word::ZERO, self);
        }
        let mut q = a / b; // truncado
        let r = a - q * b; // sinal de a (ou zero)
        if 2 * r.abs() > b.abs() {
            q += r.signum() * b.signum();
        }
        (Word(q), Word(a - q * b))
    }

    /// Dígitos balanceados, little-endian (índice 0 = trit de peso 3⁰).
    pub fn trits(self) -> [i8; TRITS] {
        let mut u = self.0 + M27; // dígitos de u na base 3 = dígitos balanceados + 1
        let mut out = [0i8; TRITS];
        for d in out.iter_mut() {
            *d = (u % 3) as i8 - 1;
            u /= 3;
        }
        out
    }

    /// Palavra a partir de dígitos balanceados little-endian. `None` se algum
    /// dígito está fora de {−1,0,+1}.
    pub fn from_trits(t: &[i8; TRITS]) -> Option<Word> {
        let mut v: i64 = 0;
        for &d in t.iter().rev() {
            if !(-1..=1).contains(&d) {
                return None;
            }
            v = v * 3 + d as i64;
        }
        Some(Word(v))
    }

    /// Campo balanceado: valor dos trits [lo, lo+len) (extensão por zero).
    #[inline]
    pub fn field(self, lo: usize, len: usize) -> i64 {
        debug_assert!(lo + len <= TRITS && len >= 1);
        let u = (self.0 + M27) / POW3[lo];
        u % POW3[len] - half(len)
    }

    /// Mínimo trit a trit (∧ de Kleene sobre a ordem −1 < 0 < +1).
    pub fn kmin(self, o: Word) -> Word {
        let (a, b) = (self.trits(), o.trits());
        let mut t = [0i8; TRITS];
        for i in 0..TRITS {
            t[i] = a[i].min(b[i]);
        }
        Word::from_trits(&t).unwrap()
    }

    /// Máximo trit a trit (∨ de Kleene).
    pub fn kmax(self, o: Word) -> Word {
        let (a, b) = (self.trits(), o.trits());
        let mut t = [0i8; TRITS];
        for i in 0..TRITS {
            t[i] = a[i].max(b[i]);
        }
        Word::from_trits(&t).unwrap()
    }

    /// Notação balanceada legível, trit de topo primeiro: '+', '0', '-'.
    pub fn to_bt_string(self) -> String {
        self.trits()
            .iter()
            .rev()
            .map(|&d| match d {
                1 => '+',
                0 => '0',
                _ => '-',
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constants() {
        assert_eq!(P27, 7_625_597_484_987);
        assert_eq!(M27, 3_812_798_742_493); // confere com fase2 (bval de 27 trits +)
        assert_eq!(half(6), 364);
        assert_eq!(half(18), 193_710_244);
    }

    #[test]
    fn wrap_window_boundaries() {
        assert_eq!(Word::wrap(M27).val(), M27);
        assert_eq!(Word::wrap(M27 + 1).val(), -M27);
        assert_eq!(Word::wrap(-M27 - 1).val(), M27);
        assert_eq!(Word::wrap(P27).val(), 0);
        assert_eq!(Word::wrap(i64::MAX).val(), Word::wrap128(i64::MAX as i128).val());
        assert_eq!(Word::wrap(i64::MIN).val(), Word::wrap128(i64::MIN as i128).val());
    }

    #[test]
    fn trits_roundtrip_and_field() {
        for v in [0, 1, -1, 5, -5, 364, -365, M27, -M27, 123_456_789_012, -987_654_321] {
            let w = Word::lit(v);
            let t = w.trits();
            assert_eq!(Word::from_trits(&t), Some(w));
            // campo inteiro = valor; composição de dois campos = valor
            assert_eq!(w.field(0, 27), v);
            assert_eq!(w.field(0, 13) + POW3[13] * w.field(13, 14), v);
        }
        assert_eq!(Word::lit(5).to_bt_string(), format!("{}+--", "0".repeat(24)));
    }

    #[test]
    fn shift_matches_spec_examples() {
        // docs/05 §3.2: 5/3 → 2 (não 1), −5/3 → −2
        assert_eq!(Word::lit(5).shift(-1).val(), 2);
        assert_eq!(Word::lit(-5).shift(-1).val(), -2);
        assert_eq!(Word::lit(7).shift(2).val(), 63);
        assert_eq!(Word::MAX.shift(27).val(), 0);
        assert_eq!(Word::MAX.shift(-27).val(), 0);
        // shl descarta o topo: M·3 ≡ −1... (M = todos +, sobe e entra 0 embaixo)
        assert_eq!(Word::MAX.shift(1).val(), M27 - 1);
    }

    #[test]
    fn divrem_centered() {
        for (a, b, q, r) in [
            (7, 2, 3, 1),    // 3,5 → empate → para zero
            (-7, 2, -3, -1), // simetria de negação
            (5, 3, 2, -1),   // coincide com shift(-1)
            (-5, 3, -2, 1),
            (8, 3, 3, -1),
            (9, 4, 2, 1),
            (11, 4, 3, -1),
            (10, 4, 2, 2),   // empate: r com o sinal de a
            (-10, 4, -2, -2),
            (10, -4, -2, 2),
            (0, 5, 0, 0),
            (5, 0, 0, 5),    // total: a = 0·0 + a
        ] {
            let (qq, rr) = Word::lit(a).divrem(Word::lit(b));
            assert_eq!((qq.val(), rr.val()), (q, r), "{a} / {b}");
        }
    }
}
