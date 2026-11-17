# Fase 4A — Auditoria matemática, semântica e diferencial da ALU ternária

Data: 2026-11-16. Escopo: auditar `src/rust/src/alu.rs` antes de ISA/
registradores/memória. Nenhum benchmark; nada de ISA implementada.

> **CORREÇÃO (2026-11-16, Fase 5):** as provas Lean citadas neste documento
> NÃO tinham sido verificadas quando ele foi escrito. O `lake build` de
> então construía só o executável (que importa apenas `Basic.lean`);
> `Phase3.lean` e `Phase4A.lean` não compilavam (20 e 34 erros). Na Fase 5
> as provas foram refeitas e hoje compilam; os enunciados foram mantidos,
> salvo `addC_spec` (a palavra `w` era quantificada como `Int`). Evidência:
> `results/phase5/lean-build-antes-da-correcao.log` e `lean-build.log`;
> detalhes em docs/09-relatorio-fase5.md (achado L1).

---

## A. Baseline

Reproduzido ANTES de qualquer modificação:

- Comando: `nix-shell shell-rust.nix --run 'cd src/rust && cargo test'`
- Resultado: **36 testes verdes** = 15 (lib) + 8 (tests/all.rs) + 13 (tests/alu.rs),
  mais dois harnesses com 0 testes (bench/doctest). Exit code 0.
- Log: `results/phase4a/f4a-baseline.log`.
- Único aviso: `unused variable: n` em `shr` (residual da edição da Fase 4;
  corrigido na 4A).
- Regressão da dependência Haskell (Fase 1): `cabal test` (QuickCheck) —
  PASS, exit 0 (`results/phase4a/f4a-fase1-regressao.log`). Nenhum teste da
  Fase 2/3 reexecutado além do necessário.

## B. Extensão de palavras

- **Semântica correta:** preenchimento com ZERO no lado alto. V′ = V.
  Prova Lean: `bvalI_append` (A8 generalizado) + `bvalI_zero_extend`.
- **Replicação refutada e demonstrada:** replicar o trit de topo t adiciona
  t·(3^to − 3^from)/2 ≠ 0 ([−1,−1] = −4 → −13). Teste
  `f4a_width_extend_replication_refuted` verifica a EQUAÇÃO do delta e que
  replicação = zero-extensão ⟺ t = 0.
- **Nomenclatura:** `sign_extend` → `width_extend` (o nome antigo incorporava
  a hipótese refutada). Dependências verificadas antes: apenas alu.rs + 2
  usos em testes. Zero `sign_extend` restantes no código.
- **Testes:** exaustivos n ≤ 3 → m ∈ {n+1, 5} (valor, canonicidade,
  domínio, preservação de cmp), PRNG 9→20, cross-check Haskell 3→6.

## C. Deslocamentos

- `shl`: ×3; **overflow ⟺ trit de topo ≠ 0** — provado em AMBAS as
  direções (Lean `shl_overflow_iff`: topo=0 ⟹ 3|V| ≤ Mₙ−1; topo=±1 ⟹
  3|V| ≥ Mₙ+2). `shl` DESCARTA o trit de topo que sai da largura.
- `shr` (= `trunc_low(a,1)`): **resultado = inteiro mais próximo de V/3,
  sem empates** — V = a₀ + 3·Q, |a₀| ≤ 1 < 3/2; provado em Lean
  (`bvalI_separation`, `lsb_drop_nearest`, `lsb_drop_unique`). Exemplos
  verificados: 5/3 → 2 (não 1), −5/3 → −2 (não −1), contra referência
  independente de arredondamento (`round_div_pow3`, que falha se houver
  empate).
- **Perda de informação: HÁ.** shl∘shr zera o LSB: V → V − a₀ (o LSB é o
  resto). shr∘shl = identidade ⟺ shl não estourou. A alegação anterior
  "sem perda" (fase3d-sintese.md, gate §6) foi CORRIGIDA com nota datada.
- Reversibilidade exaustivamente testada com condições exatas
  (`f4a_shl_shr_reversibility_conditions`): (a) shr∘shl = id ⟺ !overflow,
  com topo perdido quando estoura; (b) bval(saída de shl) = 3V ⟺ !overflow;
  (c) shl∘shr = palavra com LSB zerado.

## D. Aritmética

| Op | Contrato | Prova Lean | Testes-chave |
|---|---|---|---|
| `neg` | digitwise, sem carry, preserva faixa | `bvalI_neg`, `negI_involutive` | exaustivo n=3; −(A+B) = −A−B exaustivo n=2 + 5k PRNG n=9 |
| `add` | schoolbook, carry ∈ {−1,0,1}; overflow ⟺ carry final ≠ 0 ⟺ \|soma\| > Mₙ; entrega wrap balanceado | `balDig_spec`, `addC_spec`, tricotomia (3 teoremas) | exaustivo n=2, n=3 com flags; 20k vs i128 n∈{5,9,20,27,40} |
| `sub` | a + (−b); overflow = o da soma com −b | valor de negI b | exaustivo com flags separadas |
| `mul` | produto exato i128 (n ≤ 40), truncado, flag ⟺ \|p\| > Mₙ | unicidade do wrap (`window_unique`; laço = limitação) | identidades algébricas exaustivas n=2; n=40 dedicado (2k) |
| `cmp` | lexicográfica do topo → trit; correta em TODO o domínio | `bvalI_inj` | exaustivo n=2 (antisimetria) + 729² pares vs ordem numérica |

Multiplicação: n = 40 é SEGURO com i128 (|M₄₀|² ≈ 3,70e37 < 1,70e38) —
cobertura adicionada. A flag de distributividade só é comparável quando
B+C não estoura (com overflow de B+C a identidade vale sobre os valores
ENTREGUES, mod 3ⁿ) — precisão documentada no teste.

## E. Overflow e truncamento

- **Soma:** overflow ⟺ \|bval a + bval b\| > Mₙ ⟺ carry final ≠ 0. Tricotomia
  completa provada (positivo → carry 1, negativo → carry −1, dentro → 0).
  Casos-limite mantidos: M+M → wrap −1; M+1 → wrap −M; M+(−M) = 0.
- **Shl:** ⟺ topo ≠ 0 (provado, ambas direções).
- **Mul:** ⟺ \|produto exato\| > Mₙ (referência i128 não pode estourar: n ≤ 40).
- **Truncamento (trunc_low k):** arredondamento ao múltiplo mais próximo de
  3ᵏ, SEM EMPATES (3ᵏ ímpar). Separado de: truncamento de largura (from_i64),
  saturação (não implementada), overflow modular (flag) e arredondamento
  genérico. Exaustivo n=4, k ∈ {0..4}.
- **Extensão:** zero-fill; valor e canonicidade preservados.
- **Janela única:** Mₙ = (3ⁿ−1)/2; unicidade do representante por classe
  mod 3ⁿ provada (`window_unique`); laço do from_i64 verificado por teste
  (limitação registrada).

## F. Validação cruzada

- **Mapa semântico:** Haskell é little-endian (cabeça = LSB); btShiftL
  prefixa zeros no lado BAIXO (= ×3ᵏ ✓); btTrunc derruba k baixos (= A6 ✓);
  btAdd preenche o lado ALTO com Z (= width_extend ✓); toBT floor-mod = 
  rem_euclid do Rust ✓. Nenhuma contradição real; camadas canônicas
  (largura variável vs fixa) compatíveis — documentadas em
  docs/05-alu-especificacao.md §5.
- **Artefato independente:** `results/phase4a/haskell_canonical_width6.txt`
  (729 linhas), gerado por `experiments/phase4a/gen_haskell_words.hs` com
  APENAS `toBT` de BT.hs. Consumido por Rust via `include_str!`:
  from_i64(v,6) produz EXATAMENTE os mesmos dígitos (dois codificadores
  independentes — não uma função testando a si mesma); width_extend 3→6
  bate; cmp concorda com a ordem numérica nos 729² pares.
- **Lean:** 16 teoremas sem `sorry` (lista em docs/05 §8), importado no
  módulo raiz; conexão com Basic.lean via `bval_map_val`.
- **Nit no Haskell registrado** (não corrigido — baseline da Fase 1):
  comentário de btAdd diz s ∈ [−2,2]; na verdade s ∈ [−3,3]; o comportamento
  está correto.

## G. Correções

| Arquivo | Mudança | Motivo |
|---|---|---|
| `src/rust/src/alu.rs` | cabeçalho reescrito com contratos auditados; `sign_extend` → `width_extend` (+2 usos em testes); aviso `unused n` em `shr` eliminado; novos `trunc_low`, `valid_digits`; docs precisos de shr/shl/cmp | contradição documental + nomenclatura que incorporava hipótese refutada + higiene |
| `src/rust/tests/alu.rs` | 11 testes novos `f4a_*` (additivos; os 36 originais intactos) | §4.4/§5.5/§6/§9 do brief |
| `experiments/phase4a/gen_haskell_words.hs` | NOVO gerador do artefato | referência independente |
| `results/phase4a/haskell_canonical_width6.txt` | NOVO artefato (729 linhas) | consumido pelo teste Rust |
| `src/lean/TeneruraProofs/Phase4A.lean` | NOVO: 16 teoremas | §8.2 do brief |
| `src/lean/TeneruraProofs.lean` | import Phase4A | integração |
| `results/phase3/fase3d-sintese.md` | 2 notas de correção datadas (gate: replicação refutada; "sem perda" incorreto) | contradição documental exigida pelo brief |
| `results/phase4/fase4-verdicto.md` | nota 4A (rename + precisão shr/shl) | rastreabilidade |
| `docs/04-fase3-fase4-relatorio.md` | nota 4A (rename) | rastreabilidade |
| `docs/05-alu-especificacao.md` | NOVO | entregável §10 |

Bugs pegos durante a própria 4A (registrados como evidência): (1) formulação
incorreta inicial de que shr∘shl = id "sempre" — refutada pelo teste
exaustivo (shl descarta o topo); docs e teste corrigidos para a condição
exata. (2) Comparação ingênua de flags na distributividade quando B+C
estoura — restringida ao caso sem overflow, com a identidade sobre valores
entregues verificada separadamente. Ambos os testes agora codificam as
propriedades CORRETAS, não apenas as que passam.

## H. Resultados

- Rust: `nix-shell shell-rust.nix --run 'cd src/rust && cargo test'` →
  **47 testes** (15 lib + 8 all + 24 alu), 0 falhas, **exit 0**
  (`results/phase4a/f4a-final-cargo.log`).
- Lean: `nix-shell shell.nix --run 'lake build'` → build OK, **exit 0**,
  sem `sorry`/axiomas (`results/phase4a/f4a-final-lake.log`).
- Haskell: `cabal test` (regressão Fase 1) → PASS, exit 0; artefato gerado
  com runghc (exit 0).
- Avisos Rust: apenas os 2 preexistentes do binário bench (`WARMUP` unused,
  mut desnecessário) — fora do escopo da ALU, não alterados.

## I. Veredicto

1. **A semântica da ALU está matematicamente consistente?** SIM. Cada
   operação tem equação explícita, faixa, overflow e casos especiais;
   as propriedades centrais estão provadas em Lean e testadas (exaustivo
   em domínios pequenos, diferencial i128 e cross-check Haskell nos grandes).
2. **Existe divergência entre as três implementações?** NÃO nos pontos
   comparados (codificação, valor, neg, add, sub, mul, cmp, extensão,
   deslocamentos, overflow, truncamento). A única diferença é DELIBERADA e
   documentada: btAdd (Haskell) não tem flag de overflow e devolve forma
   canônica de largura variável; comparação feita ao nível do contrato de
   valor. Nit de comentário no Haskell registrado, sem impacto semântico.
3. **A extensão de palavras está corretamente definida?** SIM: zero-fill no
   lado alto, valor e canonicidade preservados (prova + testes + refutação
   da replicação mantida como evidência). Nome corrigido para
   `width_extend`.
4. **Os deslocamentos estão corretamente definidos?** SIM: shl = ×3 com
   overflow ⟺ topo ≠ 0 (provado nas duas direções); shr/trunc_low =
   arredondamento ao mais próximo sem empates (provado), com perda do LSB
   DOCUMENTADA e condições exatas de reversibilidade testadas e definidas.
5. **Overflow e truncamento possuem contratos suficientes?** SIM: tricotomia
   do overflow de soma provada; shl/mul com condições provadas; trunc_low
   com equação de arredondamento provada; wrap único na janela provado.
   Limitação explícita: laço do from_i64 verificado por teste, não por prova.
6. **A ALU está pronta para base de uma ISA?** SIM quanto à semântica: os
   contratos são inequívocos, preservam a separação semântica/representação
   e não antecipam decisões de ISA (cmp devolve trit, sem desvio embutido).

## J. Próxima etapa

As evidências autorizam avançar para a **especificação da ISA sobre estes
contratos** (codificação de instruções, banco de registradores de larguras
do contêiner — candidatos n=5/20/40 já identificados nas fases 2–4 — e
flags de overflow como estado visível), SEM alterar a ALU; qualquer
divergência futura contra a referência Haskell/Lean deve ser tratada como
bug de ISA, não ajuste ad hoc da ALU. Não implementar memória/pipeline
antes de a ISA estar especificada e auditada.
