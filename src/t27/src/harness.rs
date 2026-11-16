//! TENERURA T27 — apoio a testes e ferramentas no lado do HOSPEDEIRO:
//! construção de tabelas de páginas direto na RAM (para exercitar a máquina
//! nua, sem núcleo) e constantes comuns em sintaxe de montador.

use crate::machine::{page_of, Machine, PAGE_SIZE, PTE_TABLE};
use crate::word::Word;

/// Constantes da plataforma para `.include`/prefixo de programas de teste.
pub const PRELUDE: &str = "
    .equ DEV_CONSOLE, 14348907      ; 3^15: centro da página do console
    .equ DEV_POWER,   14349636      ; 3^15 + 729: dispositivo de energia
    .equ PAGE_SIZE,   729
    .equ PAGE_HALF,   364
    .equ PTE_LEAF,    1
    .equ PTE_TABLE,   -1
    .equ PTE_W,       3
    .equ PTE_X,       9
    .equ ST_KERNEL,   1             ; STATUS: trit 0 = modo
    .equ ST_IE,       3             ; trit 1 = interrupções
    .equ ST_PKERNEL,  9             ; trit 2 = modo anterior
    .equ ST_PIE,      27            ; trit 3 = IE anterior
";

/// Tabelas de páginas de 2 níveis montadas pelo hospedeiro.
/// Quadros são identificados pelo ENDEREÇO DO CENTRO (múltiplo de 729).
pub struct PageTables {
    pub root: i64,
    next_free: i64,
}

impl PageTables {
    /// `root`: centro do quadro da tabela raiz; `first_free`: centro do
    /// primeiro quadro livre para tabelas de 2º nível (cresce para cima).
    pub fn new(m: &mut Machine, root: i64, first_free: i64) -> PageTables {
        assert_eq!(root % PAGE_SIZE, 0);
        assert_eq!(first_free % PAGE_SIZE, 0);
        zero_frame(m, root);
        PageTables { root, next_free: first_free }
    }

    /// Mapeia a página virtual `vpn` no quadro de centro `frame` com `flags`
    /// (combinação de PTE_LEAF, PTE_W, PTE_X).
    pub fn map(&mut self, m: &mut Machine, vpn: i64, frame: i64, flags: i64) {
        assert_eq!(frame % PAGE_SIZE, 0, "quadro não centrado");
        let i1 = page_of(vpn);
        let i2 = vpn - i1 * PAGE_SIZE;
        let e1 = m.peek(self.root + i1).expect("raiz fora da RAM");
        let table = if e1.val() == 0 {
            let t = self.next_free;
            self.next_free += PAGE_SIZE;
            zero_frame(m, t);
            m.poke(self.root + i1, Word::lit(t + PTE_TABLE));
            t
        } else {
            e1.val() - PTE_TABLE
        };
        m.poke(table + i2, Word::lit(frame + flags));
    }

    /// Próximo quadro ainda não usado por tabelas.
    pub fn next_free(&self) -> i64 {
        self.next_free
    }
}

pub fn zero_frame(m: &mut Machine, centre: i64) {
    for a in centre - crate::machine::PAGE_HALF..=centre + crate::machine::PAGE_HALF {
        assert!(m.poke(a, Word::ZERO), "quadro fora da RAM");
    }
}
