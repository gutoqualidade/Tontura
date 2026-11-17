# Fase 5 (documentação) — Especificação consolidada da ALU ternária

**Status:** auditada na Fase 4A (2026-11-16). Relatório da auditoria:
`results/phase4a/auditoria.md`. Provas Lean: `src/lean/TeneruraProofs/Phase4A.lean`
(sem `sorry`, sem Mathlib). Testes: `src/rust/tests/alu.rs`.

> **CORREÇÃO (2026-11-16, Fase 5):** as provas Lean citadas neste documento
> NÃO tinham sido verificadas quando ele foi escrito. O `lake build` de
> então construía só o executável (que importa apenas `Basic.lean`);
> `Phase3.lean` e `Phase4A.lean` não compilavam (20 e 34 erros). Na Fase 5
> as provas foram refeitas e hoje compilam; os enunciados foram mantidos,
> salvo `addC_spec` (a palavra `w` era quantificada como `Int`). Evidência:
> `results/phase5/lean-build-antes-da-correcao.log` e `lean-build.log`;
> detalhes em docs/09-relatorio-fase5.md (achado L1).

---

## 0. Princípios

1. **A ALU é ternária na semântica.** Opera sobre dígitos balanceados
   little-endian `Vec<i8>` ∈ {−1,0,+1} (posição *i* tem peso 3^*i*;
   `word[0]` é o LSB). pack2/pack3 apenas transportam bits; nada na ALU
   depende deles.
2. **Semânticas separadas.** `ArithTrit` (aritmética balanceada, dígito =
   valor), `KleeneTrit` e `LukasTrit` (lógicas de três valores) compartilham
   o portador S3 = {M, Z, P}, mas são TIPOS distintos com tabelas distintas
   (`trit.rs`). A ALU NÃO expõe operação lógica; lógica vive em módulos
   próprios (`TernLogic.hs`, `trit.rs`) e não se mistura com aritmética.
3. **Largura fixa.** Toda operação recebe/entrega palavras de largura fixa
   n. A janela canônica de n trits é [−Mₙ, Mₙ] com Mₙ = (3ⁿ−1)/2.
4. **Overflow ≠ erro.** Overflow é estado do contrato (flag/trit), não
   exceção: a palavra entregue é sempre o wrap balanceado (único
   representante da classe mod 3ⁿ na janela — Lean `window_unique`).

## 1. Domínio e faixa

- **Domínio:** toda palavra com dígitos ∈ {−1,0,+1}. Toda palavra do domínio
  está na janela (|Σdᵢ3ⁱ| ≤ Σ3ⁱ = Mₙ — Lean `bvalI_range`) e é o
  representante ÚNICO do seu valor em largura fixa (injetividade da
  codificação — Lean `bvalI_inj`). **Não existem palavras não canônicas** no
  domínio: preenchimento com zeros altos é a MESMA palavra estendida, não
  outra representação.
- Mₙ é computado pela recorrência Mₖ₊₁ = 3·Mₖ + 1 (3ⁿ estoura i64 para n=40;
  M₄₀ = 6 078 832 729 528 464 400 não estoura). `m_max(n)` exige n ≤ 40.
- `from_i64(v, n)` / `from_i128`: v mod 3ⁿ na janela (rem_euclid + dígitos
  com correção de dígito −1). Corretude do laço: **verificada por teste**
  (congruência + unicidade + janela, `property_from_i64_window_congruence_
  uniqueness`) e pelo cross-check Haskell (§4) — não formalizada em Lean
  (limitação registrada).

## 2. Operações aritméticas

### 2.1 `neg(a) -> Vec<i8>`
- Entrada/saída: palavra de n trits, mesma largura.
- Semântica: −(Σdᵢ3ⁱ) = Σ(−dᵢ)3ⁱ. Sem carry; preserva faixa (|−V| = |V|).
- Propriedades provadas (Lean): valor (`bvalI_neg`), involução digitwise
  (−(−A) = A; `negI_involutive`), distribuição −(A+B) = (−A)+(−B) — testada
  exaustivamente n=2 e 5k propriedades n=9 (`f4a_neg_distributes_over_add`).
- Caso especial: 0 → 0. Overflow: nunca.

### 2.2 `add(a, b, n) -> (Vec<i8>, bool)`
- Entrada: duas palavras de n trits; c₀ = 0.
- Semântica: schoolbook coluna a coluna: sᵢ = aᵢ + bᵢ + cᵢ ∈ [−3,3];
  dígito dᵢ = balDigD(sᵢ), carry cᵢ₊₁ = balDigC(sᵢ) ∈ {−1,0,+1}
  (balDig: s ≥ 2 → (s−3, +1); s ≤ −2 → (s+3, −1); senão (s, 0)
  — Lean `balDig_spec`).
- Invariante provada (Lean `addC_spec`): a + b + c₀ = w + 3ⁿ·c′ com w
  palavra válida e c′ ∈ {−1,0,+1}.
- **Overflow ⟺ carry final c′ ≠ 0 ⟺ |soma exata| > Mₙ** (tricotomia
  provada: soma > Mₙ ⟺ c′ = 1; soma < −Mₙ ⟺ c′ = −1; |soma| ≤ Mₙ ⟺ c′ = 0 —
  Lean `add_overflow_iff_zero`, `add_pos_overflow`, `add_neg_overflow`).
- Sem overflow: bval(w) = a + b (exato). Com overflow: bval(w) ≡ a + b
  (mod 3ⁿ), |bval(w)| ≤ Mₙ (wrap balanceado).
- Testes: exaustivo n=2 (81 pares) e n=3 (729 pares, com flags); 20k
  propriedades vs referência i128 em n ∈ {5,9,20,27,40}.

### 2.3 `sub(a, b, n) -> (Vec<i8>, bool)`
- Semântica: a + (−b); overflow = o da soma com −b (mesma flag logicamente:
  |a − b| > Mₙ ⟺ |a + (−b)| > Mₙ). Testado exaustivamente com flags
  separadas da soma.

### 2.4 `mul(a, b, n) -> (Vec<i8>, bool)`
- Entrada: duas palavras de n trits, **n ≤ 40** (garantia de produto exato
  em i128: |M₄₀|² ≈ 3,70e37 < i128::MAX ≈ 1,70e38).
- Semântica: p = bval(a)·bval(b) (exato em i128); entrega from_i128(p, n)
  (wrap balanceado) e flag **overflow ⟺ |p| > Mₙ**.
- Propriedades testadas: comutatividade (dígitos), a·0 = 0 (sem flag),
  a·1 = a (sem flag), distributividade A(B+C) ≡ AB + AC (mod 3ⁿ) sobre os
  valores ENTREGUES (exaustivo n=2), propriedade vs i128 em n ∈
  {5,9,20,27,40} + n=40 dedicado (`f4a_mul_n40_property`, semente
  0x5EED_2026_1116 ^ 0x40A0, 2000 iterações).
- Limite: para n > 40 a referência i128 perde exatidão — uso proibido
  (assert).

### 2.5 `cmp(a, b) -> i8 ∈ {−1, 0, +1}`
- Entrada: duas palavras de MESMA largura (extensões são responsabilidade
  do chamador via `width_extend`).
- Semântica: lexicográfica do topo; sinal de a − b como TRIT (sem desvio
  embutido — conversão para controle é decisão de ISA).
- Correção para TODO o domínio: no par de dígitos de topo onde diferem
  (posição k), a diferença das caudas |·| ≤ 3ᵏ − 1 < 3ᵏ não sustenta o
  dígito de cima (injetividade — Lean `bvalI_inj`). Zeros altos não enganam
  porque a comparação é dígito a dígito, não via trit de sinal.
- Propriedades: antisimetria cmp(a,b) = −cmp(b,a), reflexividade, ordem
  total concordando com bval — exaustivas em n=2 e nos 729² pares da
  palavra Haskell (§4).

## 3. Operações de deslocamento e truncamento

### 3.1 `shl(a) -> (Vec<i8>, bool)`
- Semântica: dígitos sobem uma posição, LSB ← 0. Valor matemático alvo: 3·V.
- **Overflow ⟺ trit de topo ≠ 0** (ambas as direções provadas — Lean
  `shl_overflow_iff`: topo = 0 ⟹ |3V| ≤ 3·Mₙ₋₁ = Mₙ − 1; topo = ±1 ⟹
  3|V| ≥ (3ⁿ+3)/2 = Mₙ + 2).
- Com overflow, a SAÍDA não representa 3·V: bval(saída) = 3V − 3ⁿ·topo
  (wrap). A flag marca isso; nada é "corrigido" silenciosamente.
- `shl` descarta o trit de topo que sai da largura.

### 3.2 `shr(a) -> Vec<i8>` (= `trunc_low(a, 1)`)
- Semântica: descarta o LSB, repõe a largura com zero no topo.
- **Equação: resultado = inteiro mais próximo de V/3, SEM EMPATES.**
  V = a₀ + 3·Q (separação — Lean `bvalI_separation`); |a₀| ≤ 1 < 3/2 ⟹ Q é
  o único inteiro com |V − 3Q| ≤ 1 e todo R ≠ Q tem |V − 3R| ≥ 2
  (Lean `lsb_drop_nearest`). Exemplo verificado: 5/3 → 2 (não o truncamento
  convencional 1); −5/3 → −2 (não −1).
- **HÁ PERDA de informação:** o LSB descartado é o resto. shl∘shr zera o
  LSB: shl(shr(V)) = V − a₀. shr∘shl = identidade sobre dígitos ⟺ shl não
  estourou (o topo descartado por shl não volta). Documentado em alu.rs e
  corrigido em results/phase3/fase3d-sintese.md (nota 4A).
- Overflow: nunca (o resultado está na janela de n−1 trits — Lean
  `shr_window`).

### 3.3 `trunc_low(a, k) -> Vec<i8>` (novo, espelha `btTrunc` do Haskell)
- Pré-condição: 0 ≤ k ≤ n. Saída: mantém dígitos k..n−1, repõe a largura
  com k zeros (largura preservada).
- **Equação: resultado = inteiro mais próximo de V/3ᵏ, sem empates**
  (3ᵏ é ímpar; resto representável |V mod 3ᵏ| ≤ (3ᵏ−1)/2 < 3ᵏ/2). k = 1 é
  `shr`; k = 0 é identidade; k = n é 0.
- Testes: exaustivo n=4, k ∈ {0,1,2,3,4}, todos os 81 valores da janela,
  contra referência de arredondamento independente (`round_div_pow3`,
  que falha se houver empate); equivalência trunc_low(_,1) == shr (2k
  propriedades n=9).
- **Não confundir com:** truncamento de largura (reduzir n — via from_i64),
  saturação (não implementada), overflow modular (flag), arredondamento
  numérico genérico (este é o caso sem empates).

## 4. Extensão de largura

### 4.1 `width_extend(a, from, to) -> Vec<i8>` (renomeada de `sign_extend`)
- Pré-condição: from ≤ to, a.len() = from. Preenche o lado ALTO com ZERO.
- **Equação: preservação exata de valor** — V′ = V (Lean
  `bvalI_zero_extend`: bvalI(ds ++ zeros) = bvalI ds; apêndice no lado alto
  = posições altas com peso · 0).
- **RESULTADO NEGATIVO REGISTRADO (Fase 4, mantido):** "extensão de sinal =
  replicar o trit de topo" é REFUTADA. Todos os pesos são positivos;
  replicar t adiciona t·(3^to − 3^from)/2 ≠ 0 (ex.: [−1,−1] = −4 →
  [−1,−1,−1] = −13). Em binário em complemento-de-2 funciona porque o peso
  do bit de sinal é −2ⁿ⁻¹ (duplicá-lo ≡ 0 mod 2ⁿ). A referência Haskell
  sempre preencheu com Z — a ALU coincide com ela. Teste de refutação
  mantido: `f4a_width_extend_replication_refuted`.
- **Decisão de nomenclatura (Fase 4A):** renomear para `width_extend`
  porque o nome antigo incorporava a hipótese refutada. Dependências
  verificadas antes do rename: apenas alu.rs (def + docs) e 2 testes.
  `zero_extend` também seria correto; `width_extend` descreve o efeito
  (mudança de largura) sem inferir regra de sinal.
- Propriedades: valor, canonicidade (w estendida = from_i64(v, to)) e
  comparação preservados — exaustivo n ≤ 3 → m ∈ {n+1, 5} e PRNG 9→20;
  cross-check com Haskell (§6).

## 5. Convenções de representação (Rust ↔ Haskell ↔ Lean)

| Aspecto | Rust (alu.rs) | Haskell (BT.hs) | Lean (Phase4A) |
|---|---|---|---|
| Ordem dos dígitos | little-endian (`Vec<i8>`, [0] = LSB) | little-endian (lista, cabeça = LSB) | little-endian (cabeça = LSB) |
| Valor | Σ dᵢ·3ⁱ (pack3::bval, Horner) | btValue (Integer) | bvalI / bval (Trd) |
| Forma canônica | largura fixa, zeros altos explícitos | largura variável (corta zeros altos) | largura implícita (length) |
| Extensão | zero no lado alto | Z no lado alto (btAdd) | append de zeros (`bvalI_zero_extend`) |
| Truncamento baixo | trunc_low/shr | btTrunc (derruba k baixos) | separação + `lsb_drop_nearest` |
| shl | sobe dígitos, LSB ← 0 | btShiftL (prepend Z no lado baixo = ×3ᵏ) | `shl_overflow_iff` |
| Negação | digitwise | btNeg digitwise | negI digitwise |
| Wrap | from_i128: rem_euclid + dígito −1 | toBT: mod 3 + dígito −1 | `window_unique` (unicidade) |
| Soma | schoolbook c ∈ {−1,0,1}, flag | btAdd coluna a coluna, SEM flag | addC + tricotomia |

As duas camadas canônicas (largura fixa vs variável) são compatíveis: a
forma variável do Haskell é a forma de largura fixa do Rust com os zeros
altos removidos; o artefato de cross-check (§6) usa largura fixa 6 nos dois
lados. **Nit registrado no Haskell** (não corrigido — código da Fase 1 é
baseline): o comentário de `btAdd` afirma s ∈ [−2,2], mas s = d₁ + d₂ +
carry ∈ [−3,3] (±3 ocorre com dígito e carry opostos no limite); o COMPORTAMENTO
está correto (as duas ramificações cobrem [−3,3]).

## 6. Verificação cruzada Rust ↔ Haskell

- Artefato INDEPENDENTE: `results/phase4a/haskell_canonical_width6.txt`
  (729 linhas, "v|dígitos6" big-endian '+'/0/'-', v ∈ [−364,364]), gerado
  por `experiments/phase4a/gen_haskell_words.hs` usando APENAS `toBT` de
  BT.hs (nenhum código Rust envolvido na geração).
- Consumido por `f4a_haskell_crosscheck_width6` via `include_str!`:
  (i) dígitos válidos, bval = v, e `from_i64(v,6)` produz EXATAMENTE os
  mesmos dígitos (dois codificadores independentes); (ii) width_extend
  3→6 bate com o Haskell para todos |v| ≤ 13; (iii) cmp ordena os 729²
  pares exatamente como a ordem numérica.
- Contrato distinto deliberado: `btAdd` (Haskell) devolve forma canônica de
  largura variável SEM flag de overflow; `add` (Rust) devolve largura fixa
  COM flag. Comparação feita ao nível do contrato de valor (≈ mod 3ⁿ na
  janela), documentada na tabela acima.

## 7. Semânticas não aritméticas (fora do escopo da ALU)

- `KleeneTrit`: lógica de Kleene (T, F, U); `LukasTrit`: Łukasiewicz.
  Tabelas verificadas em `trit::tests` (baseline Fase 1/2, mantidos).
- A ALU não adquire operação lógica "para aumentar instruções": instruções
  lógicas futuras da ISA referenciarão as semânticas tipadas separadamente.

## 8. Propriedades provadas em Lean (Phase4A.lean, sem `sorry`)

| # | Teorema | Conteúdo |
|---|---|---|
| 1 | `Mn_pow3` | 3ⁿ = 2·Mₙ + 1 (recorrência da faixa; ímparidade) |
| 2 | `bvalI_append` | A8 generalizado: bvalI(xs++ys) = bvalI xs + 3^\|xs\|·bvalI ys |
| 3 | `bvalI_zero_extend` | §8.2.1 extensão por zeros no alto preserva valor |
| 4 | `bvalI_range` | §8.2.6 |palavra de dígitos válidos| ≤ Mₙ |
| 5 | `bvalI_separation` | §8.2.2 V = a₀ + 3·Q |
| 6 | `lsb_drop_nearest` | §8.2.3 queda do LSB = mais próximo, sem empates (separação ≥ 2 vs ≤ 1) |
| 7 | `lsb_drop_unique` | unicidade do quociente da queda |
| 8 | `bvalI_neg`/`negI_involutive` | §8.2.4 valor e involução da negação |
| 9 | `balDig_spec` | coluna: s = d + 3c, d,c ∈ [−1,1] |
| 10 | `addC_spec` | §8.2.5 invariante de valor + dígitos + carry da cadeia completa |
| 11 | `add_overflow_iff_zero` / `add_pos_overflow` / `add_neg_overflow` | §8.2.7 tricotomia do overflow de soma |
| 12 | `shl_overflow_iff` | §8.2.7 overflow de shl ⟺ topo ≠ 0 (ambas direções) |
| 13 | `shr_window` | shr preserva a janela |
| 14 | `bvalI_inj` | injetividade da codificação em largura fixa (fundamento de cmp) |
| 15 | `bval_map_val` | bvalI ∘ map Trd.val = bval (conexão com Basic.lean) |
| 16 | `window_unique` | unicidade do representante da janela por classe mod 3ⁿ |

**Limitações registradas** (explícitas, verificadas por teste em vez de
prova): (a) correção do laço de `from_i64`/`from_i128` (geração do wrap);
(b) o argumento mod-3 da unicidade do from_i64 está coberto pelo teste
`property_from_i64_window_congruence_uniqueness` + cross-check Haskell +
`window_unique` (a parte de unicidade que a prova formalizou).

## 9. Testes (Fase 4A)

Originais (36) mantidos verdes. Novos (11, prefixo `f4a_`):
1. `f4a_width_extend_exhaustive_value_cmp_domain` — exaustivo extensão n≤3→{n+1,5}.
2. `f4a_width_extend_replication_refuted` — refutação da replicação com equação t·(3ᵗᵒ−3ᶠʳᵒᵐ)/2.
3. `f4a_shr_example_5_and_minus5` — exemplos 5→2, −5→−2 vs truncamento.
4. `f4a_shl_shr_reversibility_conditions` — condições exatas de reversibilidade.
5. `f4a_trunc_low_is_round_to_nearest_multiple` — trunc_low exaustivo n=4, k∈0..4.
6. `f4a_mul_algebraic_identities_exhaustive_n2` — comutativa/distributiva/identidades.
7. `f4a_mul_n40_property` — largura máxima com i128 exato.
8. `f4a_neg_distributes_over_add` — −(A+B) = −A−B (exaustivo + PRNG).
9. `f4a_cmp_domain_closure_antisymmetry_exhaustive_n2` — antisimetria/ordem.
10. `f4a_haskell_crosscheck_width6` — cross-check com artefato Haskell.
11. `f4a_valid_digits_rejects_out_of_domain` — helper de domínio.

Sementes PRNG fixas: xorshift64*, 0x5EED_2026_1116 e derivadas registradas
no código. Testes exaustivos em domínios pequenos; aleatórios apenas onde
o domínio é grande (n ≥ 5).
