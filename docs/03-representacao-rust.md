# TENERURA — Relatório: representação e compactação de trits em Rust (Fase 2a)

Data: 2026-11-16 · Toolchain: rustc/cargo 1.99.0 (já presente em `~/.rustup`,
shims ausentes no PATH → `tools/rust-env.sh` + `shell-rust.nix` com gcc do Nix
para o linker) · Projeto: `~/tenerura/src/rust/` (zero dependências externas)

## 1. O que foi implementado

| Camada (§3 do plano) | Módulo | Conteúdo |
|---|---|---|
| Lógica | `trit.rs` | Portador `S3` (M/Z/P, ordem = lattice F≤U≤T) + `ArithTrit`, `KleeneTrit`, `LukasTrit` como tipos distintos; K3 (and/or/not/→ material) e Ł3 (→ de Łukasiewicz, tautologia p→p) sem derivar ops aritméticas de propósito |
| Trabalho | `pack2.rs` | 2 bits/trit; `00`=0, `01`=+1, `10`=−1, **`11` reservado detectado como `InvalidCode`**; buffer com get/set O(1); palavra de 27 trits em `u64` com bits 54..64 zerados (integridade) |
| Compacta | `pack3.rs` | Base 3 densa não-balanceada (u = d+1), little-endian; contêineres u8 (n≤5), u16 (n≤9), u32 (n≤20), u64 (n≤40) com checagem de alcance; `bval` (Σ dᵢ·3ⁱ) por Horner; rejeição explícita (nunca truncamento silencioso) |

Decisão provisória do plano respeitada: 2 bits é a representação de trabalho;
base 3 é alternativa experimental; tipos lógicos independentes da física.

## 2. Testes (17, todos executados — log `results/fase2-rust-tests.txt`, exit 0)

**Exaustivos** (domínios pequenos): 256 bytes do pack2 (roundtrip + detecção do
reservado por nibble); 243 valores do pack3-u8 n=5; 19 683 valores do pack3-u16 n=9.

**Propriedades com PRNG próprio** (xorshift64*, semente `0x5EED_2026_1116` registrada):
roundtrips 100k×(u32,n=20), 50k×(u64,n=27 e 40), 50k×(palavra 27 em u64);
**teste diferencial pack2↔pack3** (20k palavras de 27 trits: as duas codificações
transportam exatamente os mesmos trits).

**Valores fechados** de `bval`: (3²⁷−1)/2 = 3 812 798 742 493; −(3⁴⁰−1)/2 =
−6 078 832 729 528 464 400 (confere com a álgebra e com Haskell/Lean).

## 3. Bugs encontrados e corrigidos (a suíte trabalhando)

1. **Truncamento silencioso** em `pack3_u8/u16/u32`: entrada maior que o contêiner
   era truncada (`[..min(len, MAX)]`) em vez de rejeitada — perda de dados sem erro.
   Detectado pelo teste de fronteiras; corrigido para `Err(OutOfRange)`.
2. **Overflow espúrio em `bval`**: acumular peso `w` multiplicando até 3⁴⁰ estoura
   `i64` embora o valor final caiba. Corrigido com esquema de Horner.
3. **Checksum do benchmark era sempre 0** (paridade par de XORs anulava o valor):
   o "checksum" não verificava nada. Corrigido para comparar resultados entre
   rodadas e imprimir a soma real. Detecção: só depois da verificação independente
   do PRNG (somas 628/633 em programa separado) — procedimento §9 do diagnóstico
   (outlier investigado antes de aceitar o número).
4. **Erro de lógica no teste exaustivo** do pack2 (não do código): conclusão sobre
   o byte inteiro a partir do trit 0. Teste reescrito por nibble.

## 4. Benchmark (log `results/fase2-rust-bench.txt`, exit 0)

Operação: soma dos valores dos trits; N = 1 000 000; release + LTO;
1 aquecimento + 5 medições; **máquina única, rodada única — não é conclusão universal**.

| Abordagem | bytes/trit | trits/s (média) | min |
|---|---|---|---|
| A. Soma direta sobre buffer 2-bits (decode por nibble, match ramificado) | 0,250 | ~150 M | 6,50 ms |
| B. Desempacota 2-bits → Vec\<i8\> → soma | 0,250 | ~213 M | 4,23 ms |
| C. Direta sobre palavras densas base-3 (n=20 em u32, sem alocação) | 0,200 | ~1040 M | 0,93 ms |

Somas verificadas: A = B = 628 (mesmos dados; confere com verificação independente
do PRNG); C = 633 (conjunto próprio, ~0 por simetria).

**Leitura honesta (exatamente o que o plano §4.2 previu):**
- Compactação NÃO implica operações rápidas: a versão desempacotada (B) foi
  ~1,7× mais rápida que a decodificação direta ramificada (A) — a soma sobre
  `Vec<i8>` é vetorizável, o match por nibble não.
- A base 3 densa (C) venceu em ambas as dimensões *nesta operação*: 1,6 bits/trit
  (0,20 bytes/trit, ideal 1,585) e ~5× mais trits/s que A — divisão por 3
  compila para shift-multiplicação e o loop é branchless. **Não generalize:**
  soma é o melhor caso para decodificação densa; operações com carry entre
  palavras e acesso aleatório podem inverter o resultado.
- Pendências do plano §6: SIMD explícito, padrões de acesso aleatório,
  cache misses (perf), energia. Próxima rodada de benchmarks.

## 5. Status epistêmico

- **Fato verificado**: comportamento das codificações (testes exaustivos + propriedades).
- **Fato verificado em terceira via**: `bval` Rust = `btValue` Haskell = `bval` Lean
  (mesma palavra [p,m] = −2 nas três implementações independentes).
- **Resultado experimental local**: tabela de benchmark acima (máquina única).
- **Hipótese**: base 3 densa vale o custo extra de codificação/decodificação em
  operações gerais — NÃO confirmada; requer a rodada SIMD/cache/aleatória.
