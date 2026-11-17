# TENERURA — Relatório Consolidado: Fases 3A–3D e Fase 4

Data: 2026-11-16 · Baseline preservado: Fases 1 (Haskell/Lean) e 2 (Rust repr)
· Todos os experimentos com sementes/arquivos registrados em `results/phase3/`
e `results/phase4/`.

---

## A. Estado inicial

Inspecionado e preservado sem alterações:

- `docs/01-diagnostico-cientifico.md` (IDs de propriedades A1–A9/B1–B5/C1–C4),
  `docs/02-fase1-relatorio.md`, `docs/03-representacao-rust.md`.
- `src/haskell/` (BT, TernArith, TernLogic, Trit + QuickCheck) — consultado
  como referência (crucial: preenchimento com Z em TernArith.hs).
- `src/rust/` (trit.rs, pack2.rs, pack3.rs; 17 testes da Fase 2 intocados e
  re-executando verdes), `src/lean/TeneruraProofs/` (prova A8 da Fase 1).
- `results/fase1-*`, `results/fase2-*`.
- `src/lean/.git` presente; repositório Rust zero-dependências mantido.

Adaptação de layout registrada (§8): provas em `src/lean/TeneruraProofs/Phase3.lean`;
experimentos em `experiments/phase3/`; resultados em `results/phase3/` e `results/phase4/`.

## B. Fase 3A — Simetria dos números ímpares

> **CORREÇÃO (2026-11-16, Fase 5):** as provas Lean citadas neste documento
> NÃO tinham sido verificadas quando ele foi escrito. O `lake build` de
> então construía só o executável (que importa apenas `Basic.lean`);
> `Phase3.lean` e `Phase4A.lean` não compilavam (20 e 34 erros). Na Fase 5
> as provas foram refeitas e hoje compilam; os enunciados foram mantidos,
> salvo `addC_spec` (a palavra `w` era quantificada como `Int`). Evidência:
> `results/phase5/lean-build-antes-da-correcao.log` e `lean-build.log`;
> detalhes em docs/09-relatorio-fase5.md (achado L1).


**Provas Lean core (sem Mathlib), `src/lean/TeneruraProofs/Phase3.lean`, build limpo:**

| Teorema | Conteúdo |
|---|---|
| `pow3_odd` | 3ⁿ ímpar ∀n (indução + `Nat.mul_mod`) |
| `fixed_point_odd_unique` | N ímpar ∧ 2x≡0 (mod N) ⇒ x≡0 (mod N): único auto-oposto é 0 |
| `fixed_point_even_second` | N par ≥ 2: N/2 é segundo auto-oposto, distinto de 0 |
| `neg_involutive` | negação é involução |
| `balanced_half` | 3ⁿ = 2·((3ⁿ−1)/2) + 1 ∀n |
| `range_symmetry_3n` | faixa [−M, M] simétrica para TODO n — paridade de n irrelevante |

**Experimento** (`exp3a_fixed_points.py`, exaustivo N=1..64): ímpares → exatamente
1 ponto fixo; pares → exatamente 2; involução ✓; simetria da faixa n=1..12 ✓.
Confere 100% com as provas.

**Veredicto A: CONFIRMADA (matemática), com consequência arquitetural LIMITADA.**
O centro único existe por cardinalidade ímpar — vale para qualquer ímpar, não
só potências de 3. O uso concreto (votação sem empates, faixa balanceada) é
real; nenhum operador aritmético NOVO decorre da simetria. Falsa leitura
evitada por prova: simetria da faixa NÃO exige n ímpar de trits (T5).

## C. Fase 3B — Redundância e estabilidade

Arquivos: `exp3b_redundancy.py` → `exp3b_p1.csv`, `exp3b_p2.csv`, `exp3b_p3.csv`,
veredicto em `results/phase3/exp3b-verdicto.md`. Semente base 0x5EED3B01,
derivações por crc32 (`hash()` de str descartado: randomizado por processo).

**P1 (exaustivo por composições)**: correção até t=⌊(r−1)/2⌋ confirmada para
r ∈ {1..9,11,13} e os 3 símbolos; orçamento 2e+s ≤ r−1 confirmado; primeira
falha sempre em 2e+s = r; r par com empate→apagamento nunca erra, mas tem
menor capacidade por componente (r=2 falha com 1 erro onde r=3 corrige 1).
Método completo: para palavra canônica a maioria depende só da composição do
multiset — TODAS as composições verificadas (equivale a exaustivo em posições).

**P2 (Monte Carlo, 200 configs, IC Wilson 95%)**: erasure ≫ indep (15× melhor
em r=3, p=0,1); burst degrada SEMPRE a maioria (correlação local quebra a
premissa de independência); assimetria de destino é irrelevante para a
contagem. Comparação honesta por componente transportado: ganho real e
decrescente (r=1: 6,6%/comp; r=3: 0,42%/comp; r=5: 0,052%/comp a p=0,1).

**P3 (multiníveis exato)**: guardas NÃO aumentam P_ok (igual para todo L,
depende só de σ); convertem substituição em apagamento (σ=0,5: 21,2% sub em
L=3 → 0,18% em L=5 → 0% em L=7+).

**Veredicto B: PARCIALMENTE CONFIRMADA.** A parte real da hipótese ímpar é
"cardinalidade ímpar elimina empates e dá t=(r−1)/2" — propriedade do código
de repetição + paridade, com vantagem concreta do ímpar sobre o par. O resto
(maioria sempre ótima, ímpar ⇒ robusto) NÃO se sustenta: burst degrada,
apagamento vale mais que paridade. Robustez física: NÃO demonstrada (amplitude
abstrata, sem dispositivo).

## D. Fase 3C — Hierarquia numérica

Arquivos: `exp3c_hierarchy.py` → `exp3c_c1_familias.csv`, `exp3c_c2_arvores.csv`,
`exp3c_c3_packing.csv`; veredicto em `results/phase3/exp3c-verdicto.md`.

- **Exclusivo das potências de 3**: aninhamento ternário sem resíduo em todos
  os níveis (n%3ᵏ = 0 ∀k). Compostos com fator 3 (15, 21, 33, 45, 63) aninham
  1 nível e deixam subgrupo não-ternário; primos > 3 são átomos; Mersennes
  grandes não contribuem (511=7·73, 2047=23·89, resto primo; a propriedade
  "máscara all-1" é binária, sem papel ternário demonstrado).
- **Compartilhado por todos os ímpares**: faixa balanceada/centro por nível
  (T5 da 3A) — todas as 11 árvores de 729 preservam simetria.
- **Conflito real**: contagem de ESTADOS quer 3ⁱ; contagem de TRITS por
  contêiner binário quer convergentes de log₂3 — n=5 (byte: 8 bits, 0,94%
  desperdício), n=10, 20, 40, 80; n=41 precisa de 65 bits (u128). Potências
  de 3 como larguras de contêiner (n=3→5 bits, n=9→15 bits) são mediocres.
- Distinção metodológica registrada: 5 ESTADOS não são um número inteiro de
  trits (log₃5 ≈ 1,465) — os dois eixos não são comensuráveis.

**Veredicto C: PARCIALMENTE CONFIRMADA → hierarquia HETEROGÊNEA.** Níveis de
estados em 3ⁱ; larguras de contêiner/registrador por otimização de empacotamento.
Custo de endereçamento em radix mista: NÃO medido (fica em aberto, não foi
inventada uma métrica composta para esconder isso).

## E. Fase 3D — Síntese

Matriz completa e modelos em `results/phase3/fase3d-sintese.md`.

| Hipótese | Matemática | Experimento | Consequência arquitetural | Veredicto |
|---|---|---|---|---|
| A — Simetria | ✔ (T1–T5 Lean) | ✔ (exaustivo) | Parcial (negação digitwise, truncamento=arredondamento) | Confirmada, valor prático limitado |
| B — Estabilidade | Parcial (código, não paridade) | ✔ (MC 4 modelos) | Parcial (transporte opcional) | Parcialmente confirmada |
| C — Hierarquia | ✔ (estrutural) | ✔ (determinístico) | Conflito estados×contêiner; ISA pendente | Parcialmente confirmada |
| D — Combinação | ✗ | núcleo comum trivial | não formulável sem medir custo de C | Refutada como "princípio único" |

**Não existe um princípio arquitetural único unificando A+B+C.** Existe um
fato-base honesto — *cardinalidade ímpar ⇒ centro único* (prova de 4 linhas) —
que alimenta usos concretos distintos (votação, faixa por nível, aninhamento).
As propriedades são majoritariamente INDEPENDENTES entre si; duas interações
positivas (centro→votação; ímpar por nível→faixa) e um conflito interno em C.

## F. Fase 4 — Primeira ALU ternária simulada (EXECUTADA)

**Gate (§6): PASSOU** — as 6 condições verificadas em `results/phase3/fase3d-sintese.md`;
hipóteses abertas (custo físico de B, custo de endereçamento de C) isoladas da
semântica da ALU.

Implementado em `src/rust/src/alu.rs` (representation-agnostic: dígitos
little-endian, nada depende de pack2/pack3): `neg`, `add` (carry ∈ {−1,0,1},
overflow = carry final ≠ 0, wrap balanceado), `sub`, `mul` (exato i128,
truncado, flag), `cmp` (→ trit, sem desvio embutido), `shl` (overflow ⟺ topo
≠ 0, demonstrável), `shr` (÷3 arredondado — A6), `sign_extend`, `from_i64`,
`m_max`. Testes: 36 no crate, todos verdes (`results/phase4/alu-tests.txt`):
exaustivos n≤3, diferencial vs referência i128 independente em n ∈
{5,9,20,27,40}, A6 em forma correta, casos-limite; PRNG xorshift64* semente
0x5EED_2026_1116.
> **NOTA DA FASE 4A (2026-11-16):** `sign_extend` foi renomeado para
> `width_extend` (o nome antigo carregava a hipótese refutada de replicação
> de sinal); a semântica (preenchimento ZERO) não mudou. Ver
> docs/05-alu-especificacao.md.

**Descoberta da fase (resultado negativo registrado)**: a suposição "extensão
de sinal = replicar trit de topo" foi REFUTADA pelos testes — no ternário
balanceado os pesos são todos positivos e replicar muda o valor ([−1,−1] = −4
→ [−1,−1,−1] = −13). A referência Haskell (Fase 1) sempre preencheu com Z;
corrigido para preenchimento com zero, que preserva valor e canonicidade.
Isso valida o procedimento §7.4 (testar contra referência independente).

**Limitações**: `mul` exato garantido só até n=40 (i128); K3/Ł3 NÃO foram
misturadas à ALU (são semânticas de trit, já testadas em trit.rs — o §7.2
proíbe a mistura); nada de ISA/registradores/memória foi antecipado (§7.5);
nenhuma afirmação sobre hardware físico.

## G. Veredicto geral (§10 — respostas diretas)

**1. Existe vantagem matemática relevante em privilegiar números ímpares?**
SIM, mas é preciso dizer exatamente qual: cardinalidade ímpar garante centro
único auto-oposto (T2, provado) e faixa balanceada simétrica (T5, provado),
elimina empates em votação por maioria e dá capacidade de correção
t=⌊(r−1)/2⌋. NÃO existe vantagem exclusiva das potências de 3 nessas
propriedades — elas valem para todo ímpar. O que é exclusivo de 3ⁱ é o
aninhamento ternário sem resíduo (C1).

**2. Essa vantagem se traduz em robustez experimental?**
PARCIALMENTE, com condições: a maioria ternária reduz a taxa de erro
conforme previsto (exaustivo + MC), e o r ímpar bate o r par com a mesma
regra de empate. MAS ruído correlacionado degrada o ganho (burst sempre pior
que independente) e a comparação honesta por componente transportado mostra
ganhos decrescentes. Apagamentos são mais valiosos que a paridade de r
(15× no cenário testado). "Ímpar ⇒ estável" sem qualificação: FALSO.

**3. Existe uma família numérica especialmente interessante?**
SIM, duas, para papéis DIFERENTES (hierarquia heterogênea): potências de 3
para níveis de ESTADOS (único aninhamento ternário sem resíduo, profundidade
arbitrária) e as larguras derivadas dos convergentes de log₂3 — n = 5, 10,
20, 40, 80 trits — para CONTÊINERES binários (n=5 no byte tem 0,94% de
desperdício). Nenhuma família única vence em todos os papéis; primos > 3 são
átonos para hierarquia; Mersennes não contribuem ternariamente.

**4. A combinação justifica um princípio arquitetural?**
NÃO como princípio único. O que existe é um fato-base comum (centro ímpar)
com aplicações independentes e um conflito interno (estados × contêiner).
A hipótese D foi refutada na forma forte; na forma fraca ("propriedades
coexistentes e mutuamente compatíveis") é trivialmente verdadeira. Não forcei
um resultado positivo — e o custo de endereçamento em radix mista, pré-requisito
para qualquer reivindicação mais forte, continua sem medir.

**5. O que deve ser adotado na Tenerura?**
(a) Semântica balanceada já implementada: faixa [−(3ⁿ−1)/2, (3ⁿ−1)/2] para
TODO n, negação digitwise, truncamento = arredondamento sem empates (A6),
extensão de largura com ZERO (nunca replicação de topo). (b) Regras de
overflow da ALU (carry final; shl ⟺ topo ≠ 0; wrap balanceado). (c) Níveis
de estados em 3ⁱ onde aninhamento importa. (d) Larguras de contêiner n=5
(byte), 20/40 (u32/u64) onde empacotamento importa. (e) Repetição ímpar +
maioria como mecanismo OPCIONAL de canal não-confiável, com apagamentos
preferidos a substituições.

**6. O que deve permanecer experimental?**
(a) Endereçamento em radix mista (custo nunca medido — bloqueia decisões de
ISA). (b) Redundância em presença de correlação real (modelo burst é grosseiro).
(c) Multiníveis: guardas convertem erro em apagamento, mas o custo de níveis
físicos extras (energia, área, ruído entre níveis) é desconhecido. (d)
Benchmarks de operações gerais da ALU sobre pack2 vs pack3 (a Fase 2 mediu só
uma operação; carry entre palavras não medido). (e) SIMD e padrões de acesso
aleatório.

**7. O que ainda não pode ser afirmado sobre hardware físico?**
NADA sobre viabilidade física foi demonstrado — nem a favor, nem contra.
As simulações usam modelos de amplitude/ruído abstratos (gaussiano, taxa
constante, rajadas locais); nenhum dispositivo (CMOS multi-nível, memristor,
RTD) foi modelado com parâmetros reais; taxas de erro p foram parâmetros de
varredura, não medições. A propriedade matemática "centro único" não implica
estabilidade de nível elétrico algum. Qualquer salto para afirmação física
seria exatamente o erro de categoria que o §1 separa.

## H. Próximos passos (ordenados por dependência)

1. **Medir custo de endereçamento radix mista vs base-3 pura** (Rust, bench
   de decode de endereço sobre as árvores do C2; comando: `nix-shell
   ~/tenerura/shell-rust.nix --run 'cd ~/tenerura/src/rust && cargo test'`;
   novo módulo `src/rust/src/addr.rs`). Desbloqueia: decisões de ISA (§7.5).
2. **Benchmarks ALU × representação** (add/mul sobre pack2 vs pack3 vs
   dígitos, n=27/40; inclui carry entre palavras) — estende `src/rust/src/bin/bench.rs`.
3. **Modelo de ruído correlacionado mais realista** (rajadas paramétricas em
   comprimento; sensibilidade em `experiments/phase3/`) — qualifica o veredicto B.
4. **ISA mínima candidata** (só depois de 1): codificar as decisões
   já justificadas (carry ternário, flags, cmp→trit) num documento
   `docs/05-isa-candidata.md`, sem implementar microarquitetura.
5. **Formalização Lean adicional**: provar em core o shl-overflow ⟺ topo ≠ 0
   e a unicidade da janela balanceada (extensão natural de Phase3.lean).

Arquivos-chave: `docs/04-fase3-fase4-relatorio.md` (este), `src/lean/TeneruraProofs/Phase3.lean`,
`experiments/phase3/exp3{a,b,c}_*.py`, `results/phase3/*.csv|*.md`,
`src/rust/src/alu.rs`, `src/rust/tests/alu.rs`, `results/phase4/*`.
Comandos: provas `cd ~/tenerura/src/lean && lake build`; Rust `nix-shell
~/tenerura/shell-rust.nix --run 'cd ~/tenerura/src/rust && cargo test'`;
Python `nix-shell ~/tenerura/shell.nix --run 'cd ~/tenerura && python3
experiments/phase3/<exp>.py'`.
