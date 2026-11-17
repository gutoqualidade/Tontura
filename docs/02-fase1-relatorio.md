# TENERURA — Relatório Fase 1 (semântica formal + primeiras provas)

Data: 2026-11-16 · Ambiente: GHC 9.10.3 (shell.nix), Lean 4.24.0, QuickCheck 2.15

## 1. O que foi construído

### Biblioteca Haskell de referência (`~/tenerura/src/haskell/`)

- `Trit.hs` — portador `S3` (M/Z/P) e três **tipos distintos** sobre ele:
  `ArithTrit`, `KleeneTrit`, `LukasTrit` (princípio B4: semânticas separadas por tipo;
  conversões explícitas e totais; sem instância `Num` parcial).
- `BT.hs` — palavras ternárias balanceadas canônicas (little-endian), valor ↔ palavra,
  negação, sinal pelo trit mais alto, truncamento.
- `TernArith.hs` — soma com carry ∈ {−1,0,1}, produto por shift-add, shift, truncamento.
- `TernLogic.hs` — Kleene K3 (∧/∨/¬/→ material) e Łukasiewicz Ł3 (→ de Łukasiewicz).

### Testes (`test/Properties.hs`) — TODOS EXECUTADOS, NÃO APENAS ESCRITOS

Resultado do `cabal test` (log: `results/fase1-cabal-test.log`):

| Propriedade | Casos | Resultado |
|---|---|---|
| A1 roundtrip (toBT∘value = id) | 1000 | ✅ |
| A1 unicidade canônica | 1000 | ✅ |
| A3 associatividade + | 1000 | ✅ |
| A3 comutatividade + | 1000 | ✅ |
| A3 comutatividade × | 1000 | ✅ |
| A4 distributividade | 1000 | ✅ |
| A5 identidade | 1000 | ✅ |
| A5 inverso | 1000 | ✅ |
| A6 trunc = round-nearest sem empates | 1000 | ✅ |
| A8 shift = ×3^k | 1000 | ✅ |
| A9 monotonicidade (condicional a ≤ b) | 1000 (903 descartados pela pré-condição) | ✅ |
| C4 sinal pelo trit mais alto | 1000 | ✅ |
| B1 De Morgan K3 (exaustivo) | 9 | ✅ |
| B1 lattice K3 (exaustivo) | 27 | ✅ |
| B2 K3 sem 3º excluído (contraexemplo U) | exaustivo | ✅ |
| B3 Ł3: p→p tautologia (exaustivo) | 3 | ✅ |
| B4 K3 ≠ Ł3 na implicação (exaustivo) | — | ✅ |

**Seeds:** QuickCheck rodou com sementes do sistema (não fixadas); para reprodução de
qualquer contraexemplo, registrar a seed antes de reruns. Limitação: A1/A3 testam
até 12 trits (fronteira de exaustividade do §7 do diagnóstico).

### Prova Lean (`~/tenerura/src/lean/`) — verificada pelo kernel

- `bval_shift` (**A8**): inserir k dígitos z na frente multiplica o valor por 3^k.
  Prova construtiva por indução, `Int.pow_succ` + AC de multiplicação. Build sem
  `sorry` (verificado por `lake build`, log `results/fase1-lean-build.log`).

### Verificação cruzada entre implementações independentes

- Haskell `BT "+-"` (big-endian display) = **−2**; Lean `bval [p, m]` = **−2**.
  Duas implementações independentes (quickcheck-testada e kernel-provada) concordam.

## 2. Bugs encontrados e corrigidos durante a fase (prova de que a suíte trabalha)

1. **C4 — bug real de implementação**: `btSign` lia o trit da posição 0 (least
   significant) em vez do mais significativo; contraexemplo encontrado pelo QuickCheck
   em 4 testes (`-+` = −2, sinal devolvido +1). Corrigido e re-verificado.
2. **A9 — propriedade mal formulada**: faltava o antecedente a ≤ b; reescrita
   condicional (`==>`), 903/1000 casos descartados pela pré-condição (esperado).
3. Sintaxe: compreensão de lista Haskell aceita uma única expressão antes do `|`
   (encontrado via compilação, corrigido combinando com `&&`).

## 3. O que NÃO foi feito (honestidade)

- Provas de A1–A7, A9, B1–B4 continuam **testadas, não provadas formalmente** —
  status por propriedade: testada ✅ / provada ❌ (apenas A8 provada).
- Nenhuma prova em Agda ainda (Agda 2.8 instalado; formalização redundante fica para
  depois da biblioteca estabilizar, conforme plano §8.3 do diagnóstico).
- Nada físico; tudo modelo matemático puro.
- QuickCheck sem seeds fixadas nesta primeira rodada (limitação registrada acima).

## 4. Próximos passos (Fase 1 cont. → Fase 2)

1. A1 em Lean/Agda (unicidade — indutiva sobre trits, usará a injeção do algoritmo `toBT`).
2. A6 em Lean (trunc = round: |erro| < 3^k/2 — induction + divMod lemmas).
3. `ALU.hs` (Fase 4 do plano) e testes diferenciais contra `Integer`.
4. E1/E2 em Python (margem×BER Monte Carlo) com seeds fixas e logs em `results/`.
