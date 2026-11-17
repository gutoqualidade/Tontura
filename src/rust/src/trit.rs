//! TENERURA — tipos de trit com semântica explícita.
//!
//! Princípio B4 do diagnóstico: o portador físico {-1,0,+1} é compartilhado,
//! mas as semânticas (aritmética, Kleene, Łukasiewicz) são TIPOS DISTINTOS.
//! A representação compactada (pack2/pack3) é camada separada: ela transporta
//! o portador, nunca a semântica.

/// Portador físico: os três estados distinguíveis.
/// Ordem derivada M < Z < P = ordem do lattice lógico F ≤ U ≤ T.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum S3 {
    /// −1 (nível inferior)
    M,
    /// 0 (referência)
    Z,
    /// +1 (nível superior)
    P,
}

impl S3 {
    /// Valor como inteiro: M = −1, Z = 0, P = +1.
    #[inline]
    pub fn val(self) -> i8 {
        match self {
            S3::M => -1,
            S3::Z => 0,
            S3::P => 1,
        }
    }
}

/// Semântica ARITMÉTICA: dígito ternário balanceado.
/// Não deriva `Ord`/`Add` de propósito: soma de trits não é trit (tem carry).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ArithTrit(pub S3);

impl ArithTrit {
    #[inline]
    pub fn val(self) -> i8 {
        self.0.val()
    }
}

/// Semântica LÓGICA de Kleene (K3): Falso, Indeterminado, Verdadeiro.
/// Orde F ≤ U ≤ T (lattice); ∧ = min, ∨ = max, ¬ forte.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct KleeneTrit(pub S3);

impl KleeneTrit {
    /// ∧ de Kleene (forte): Falsa absorve; senão min do lattice F ≤ U ≤ T.
    #[inline]
    pub fn and(self, b: Self) -> Self {
        // M (F) < Z (U) < P (T): min e max do derivado PartialOrd/Ord coincidem
        // com as tabelas de Kleene porque S3::M < S3::Z < S3::P.
        if self.0 == S3::M || b.0 == S3::M {
            KleeneTrit(S3::M)
        } else {
            std::cmp::min(self, b)
        }
    }

    /// ∨ de Kleene (forte).
    #[inline]
    pub fn or(self, b: Self) -> Self {
        if self.0 == S3::P || b.0 == S3::P {
            KleeneTrit(S3::P)
        } else {
            std::cmp::max(self, b)
        }
    }

    /// ¬ forte: inverte F↔T, U permanece U.
    #[inline]
    pub fn not(self) -> Self {
        KleeneTrit(match self.0 {
            S3::M => S3::P,
            S3::Z => S3::Z,
            S3::P => S3::M,
        })
    }

    /// Implicação material de Kleene: ¬a ∨ b.
    #[inline]
    pub fn impl_k(self, b: Self) -> Self {
        self.not().or(b)
    }
}

/// Semântica LÓGICA de Łukasiewicz (Ł3): Falso, Possível, Verdadeiro.
/// Tipo distinto de `KleeneTrit` de propósito: a implicação difere (B4).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct LukasTrit(pub S3);

impl LukasTrit {
    /// Implicação de Łukasiewicz: min(1, 1−a+b) → em inteiros: min(2, 2−a+b)
    /// com F=0, P=1, T=2. Propriedade B3: p→p é tautologia.
    #[inline]
    pub fn impl_l(self, b: Self) -> Self {
        let a = self.0.val() as i32 + 1; // 0, 1, 2
        let b = b.0.val() as i32 + 1;
        let v = (2 - a + b).min(2);
        LukasTrit(match v {
            0 => S3::M,
            1 => S3::Z,
            _ => S3::P,
        })
    }

    /// ¬ forte.
    #[inline]
    pub fn not(self) -> Self {
        LukasTrit(match self.0 {
            S3::M => S3::P,
            S3::Z => S3::Z,
            S3::P => S3::M,
        })
    }
}

/// Converte um i8 ∈ {−1, 0, 1} em portador. Total: valores fora do conjunto
/// seguem a convenção do sinal (negativo→M, zero→Z, positivo→P); usar apenas
/// com valores no domínio (a checagem forte vive nas camadas de empacotamento).
#[inline]
pub fn s3_of_val(v: i8) -> S3 {
    match v {
        x if x < 0 => S3::M,
        0 => S3::Z,
        _ => S3::P,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kleene_matches_kleene_tables() {
        // Tabela de Kleene verificada contra a TernLogic.hs (referência testada)
        let f = KleeneTrit(S3::M);
        let u = KleeneTrit(S3::Z);
        let t = KleeneTrit(S3::P);
        // ∧
        assert_eq!(f.and(u), f);
        assert_eq!(u.and(u), u);
        assert_eq!(t.and(u), u);
        assert_eq!(t.and(t), t);
        // ∨
        assert_eq!(f.or(u), u);
        assert_eq!(t.or(u), t);
        // ¬
        assert_eq!(u.not(), u);
        // B2: a→a em K3 com a=U dá U, não T (K3 não tem o 3º excluído)
        assert_eq!(u.impl_k(u), u);
        assert_eq!(f.impl_k(f), t);
    }

    #[test]
    fn lukas_impl_is_tautological() {
        // B3: p→p em Ł3 sempre T (diferente de K3!)
        for s in [S3::M, S3::Z, S3::P] {
            assert_eq!(LukasTrit(s).impl_l(LukasTrit(s)), LukasTrit(S3::P));
        }
        // B4: p→p com p=U difere entre as lógicas: K3 dá U, Ł3 dá T
        assert_eq!(KleeneTrit(S3::Z).impl_k(KleeneTrit(S3::Z)), KleeneTrit(S3::Z)); // K3: U → U = U
        assert_eq!(LukasTrit(S3::Z).impl_l(LukasTrit(S3::Z)), LukasTrit(S3::P)); // Ł3: P → P = T
    }
}
