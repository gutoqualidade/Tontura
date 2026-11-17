# Fase 4 — Veredicto (resumo técnico)

Implementação: `src/rust/src/alu.rs` · Testes: `src/rust/tests/alu.rs` (13) +
6 testes unitários no módulo · Log: `results/phase4/alu-tests.txt` (36 testes
no total do crate, todos verdes, exit 0).

## Operações implementadas (todas sobre dígitos balanceados little-endian)

| Op | Definição | Overflow |
|---|---|---|
| `neg` | negação digitwise (sem carry) | nunca (preserva faixa) |
| `add` | schoolbook, carry ∈ {−1,0,1} | carry final ≠ 0; resultado = wrap balanceado |
| `sub` | `add(a, neg(b))` | idem |
| `mul` | produto exato em i128 (n≤40), truncado a n trits | \|produto\| > M |
| `cmp` | lexicográfica do topo → trit ∈ {−1,0,+1} | — (trit, não desvio) |
| `shl` | ×3, LSB ← 0 | ⟺ trit de topo ≠ 0 (demonstrável) |
| `shr` | ÷3 por truncamento balanceado = arredondamento (A6, sem empates) | nunca (preserva faixa) |
| `sign_extend` | preenchimento com ZERO | — |
| `from_i64`/`m_max` | janela balanceada única [−M, M] | — |

## RESULTADO NEGATIVO IMPORTANTE — suposição refutada

A suposição herdada do plano — **"extensão de sinal = replicar o trit de
topo"** — foi REFUTADA pelos testes: no ternário balanceado todos os pesos são
positivos, então replicar t adiciona t·(3ᵗᵒ−3ᶠʳᵒᵐ)/2 ≠ 0 ao valor (ex.:
[−1,−1] = −4 estendido por replicação → [−1,−1,−1] = −13). Em binário funciona
só porque o peso do bit de sinal é negativo. A referência Haskell da Fase 1
(TernArith.hs) sempre preencheu com Z; a ALU usa preenchimento com ZERO, que
preserva valor E canonicidade. Registrado em `sign_extend` (doc) e corrigido
em `shr`.

> **NOTA DA FASE 4A (2026-11-16):** (1) a função `sign_extend` foi RENOMEADA
> para `width_extend` — o nome antigo incorporava a hipótese refutada de
> replicação de sinal; dependências verificadas (alu.rs + 2 testes). Ver
> docs/05-alu-especificacao.md. (2) Precisão sobre `shr`: "arredondamento
> (A6, sem empates)" está correto, mas HÁ perda de informação — shl∘shr zera
> o LSB (V → V − d₀; o LSB descartado é o resto) e shr∘shl = identidade
> ⟺ shl não estourou. A coluna "nunca (preserva faixa)" refere-se à janela
> do RESULTADO, que de fato nunca sai de [−M, M].

## Validação (§7.4)

- **Exaustivo**: n≤2 pares add/sub/mul (81 pares), n≤3 neg/shr/shl (27 valores),
  n=3 add/sub/cmp (729 pares com flags).
- **Diferencial**: referência i128 independente (aritmética exata, algoritmo
  distinto do schoolbook) para add/sub/mul em n ∈ {5,9,20,27,40}, 20k iterações.
- **A6** verificado em sua forma correta: congruência mod 3ⁿ + janela única +
  quociente arredondado sem empates (2r = 3ⁿ impossível, 3ⁿ ímpar) + shr = ÷3
  arredondado exaustivo n≤3 e 20k propriedades.
- **Casos-limite**: zero, ±M, M+M (wrap −1), M+1 (wrap −M), M+(−M) = 0.
- **PRNG**: xorshift64*, semente 0x5EED_2026_1116 e derivadas (fixas).

## Bugs pegos pelos testes durante o desenvolvimento

1. Suposição de extensão por replicação refutada (acima).
2. `m_max`/`from_i64` computavam 3⁴⁰ em i64 → overflow em debug (3⁴⁰ ≈ 1,2e19
   > i64::MAX; M₄₀ ≈ 6,08e18 cabe). Corrigido com recorrência Mₖ₊₁ = 3Mₖ+1
   e delegação i128.
3. Subtração dupla no gerador de amostras do teste (v ∈ [−2M, 0] em vez de
   [−M, M]) — expôs que o teste de A6 estava mal formulado; reformulado.

## Limitações

- `mul` exato garantido só até n=40 (i128); n>40 exigiria big-int.
- Operações lógicas K3/Ł3 NÃO foram adicionadas à ALU: elas são operações
  sobre TRITS de semântica distinta (trit.rs, já implementado e testado na
  Fase 2), não sobre palavras aritméticas — misturá-las aqui seria exatamente
  o que o plano §7.2 proíbe. Conversões explícitas permanecem na camada de
  tipos (ArithTrit/KleeneTrit/LukasTrit).
- Sem registradores/memória/pipeline (§7.5): nada de ISA foi antecipado.
  Candidatos a fundamentar ISA futura: carry ternário ∈ {−1,0,1}, flag de
  overflow por carry final, cmp→trit (com decisão de desvio pendente),
  deslocamento ×3/÷3 como nativo.
- Simulação em Rust binário: NENHUMA afirmação sobre hardware físico.
