# TENERURA — Diagnóstico Científico e Técnico Inicial

Data: 2026-11-16 · Status: rascunho sujeito a revisão · Convenção de rótulos epistêmicos:
**[F]** fato estabelecido (com fonte conhecida) · **[D]** demonstrável/demonstrado formalmente
· **[X]** resultado experimental (publicado, de terceiros) · **[H]** hipótese a testar
· **[E]** estimativa qualitativa · **[Q]** questão em aberto

Este documento NÃO presume que a arquitetura imaginada está correta. Ele estabelece o
território, separa o que se sabe do que se supõe e propõe uma ordem de investigação.

---

## 1. Decomposição em camadas de uma arquitetura ternária genuína

Cada camada só é "genuinamente ternária" se sua semântica for definida nela mesma, não como
emulação sobre binário. A coluna "dependência binária residual" registra onde, hoje, toda
implementação realista ancora-se em infraestrutura binária.

| # | Camada | Objeto central | Dependência binária residual | Observação |
|---|--------|----------------|------------------------------|------------|
| 1 | Semântica matemática | Definições formais (trit, palavra, operação, estado) | Nenhuma | Pura matemática; provável em Lean/Agda [D] |
| 2 | Codificação de dados | Balanced ternary, trit packing, EEC ternária | Pode ser *sobre* binário (nível 1 do brief) | Distinção obrigatória: codificar ≠ computar |
| 3 | Portas lógicas | Complemento ternário, MIN/MAX, tgate | Alta: não existe processo CMOS nativo ternário [F] | Protótipos CML/STI existem em pesquisa [X] |
| 4 | Registradores/memória | Célula de 1 trit estável (biestável ternário) | Alta: memória multinível ≠ memória lógica estável | MLC provam retenção, não estabilidade lógica [F] |
| 5 | ALU | Somador ternário com carry ∈ {−1,0,1}, shifter ternário | Média | Setun (1958) provou viabilidade histórica em relés [F] |
| 6 | Barramento/interconexão | 3 níveis de sinal, 1.585 bits/trit teórico | Alta: PHYs, DDR, PCIe são binários | Custo de conversão domina [E] |
| 7 | ISA | Instruções aritméticas/lógicas/comparação ternárias | Média | Comparação por sinal (sign) é vantagem natural do BT [F] |
| 8 | Microarquitetura | Pipeline, forwarding, predição | Média | Inexplorado; nenhuma literatura densa [Q] |
| 9 | Física/dispositivos | 3 estados distinguíveis estáveis, escrevíveis, legíveis | — | Camada-raiz; define tudo acima |
| 10 | Sistema operacional | Páginas, escalonamento, IPC sobre word ternária | Média | Última camada; só vale a pena após 1–8 validadas |

**Tese central do diagnóstico [H]:** o gargalo não é a matemática (camada 1 é bem conhecida),
nem a codificação (camada 2 é trivial), mas as camadas 3, 4 e 9 — a estabilidade física de um
trit *lógico* com margem de ruído e custo competitivos. Todo o plano de pesquisa converge para aí.

### Relações transversais que precisam manter coerência semântica

- Aritmética ↔ lógica: **tipos diferentes** (ver §4 e §8). Misturá-las é a principal fonte de ambiguidade [H].
- Memória ↔ lógica: uma célula MLC não é uma porta; escrever/ler precisa de margem que a lógica destrói.
- ISA ↔ microarquitetura: a comparação ternária nativa (−/0/+ por trit) muda o desenho de branches [H].
- Física ↔ semântica: ruído, temperatura e envelhecimento mapeiam para *qual* semântica de erro? (flip de trit vs. indeterminação) [Q].

---

## 2. Matriz comparativa dos dez modelos neurais

Estimativas de custo são qualitativas (**[E]**): C (computação), M (memória/área), En (energia).
"Adaptação ternária" descreve a hipótese de trabalho, não um resultado.

| Modelo | Equações/ops dominantes | Estados naturais | Adaptação ternária proposta | Realização física plausível | Custo T vs binário | Status epistêmico |
|---|---|---|---|---|---|---|
| **Perceptron clássico** | s = Σ wᵢxᵢ; Q(s) | pesos reais; saída binária | Q₃ com zona morta ±θ (ver brief) | Comparadores de 3 níveis; janela de corrente | C: menor de todos; M: ↓ (pesos {−1,0,+1}); En: ↓ com skip de zeros | Mapeamento mais limpo; circuito direto [H], vantagem de custo [E] |
| **MLP/ReLU** | matmul + max(0,·) | pesos reais | pesos ternários {−1,0,+1} → matmul = add/sub/skip | MAC com aditivos; acumulador ainda binário | C: matmul ↓×(pesos); En: skip de zeros ↓ | TWN/TTQ mostram retenção de acurácia razoável [X]; normalização e ativações permanecem contínuas [F] |
| **CNN** | convolução deslizante | pesos compartilhados | kernels ternários + esparsidade | acelerador convolucional aditivo | M: ↓ com pesos 1-trit; En: ↓ | Idem MLP [X]; viés de convolução ajuda esparsidade [H] |
| **Transformer/attn** | QKᵀ, softmax, MLP | pesos reais, atenção contínua | pesos ternários (linha BitNet 1.58b) | igual MLP; **softmax e normalização não ternarizam** [F] | C/En: ↓ nos matmuls; attn normalização intocada | BitNet b1.58: pré-print com resultados em escala [X]; promissor mas o gargalo é o softmax [Q] |
| **LSTM** | gates σ/tanh, célula contínua | estado de célula real | quantizar gates em 3 níveis | improvável: gates precisam de dinâmica fina | En: ↓ marginal; risco de destruir memória de longo prazo | Adaptação provavelmente prejudicial [H]; a confirmar |
| **GRU** | gates σ, estado real | idem LSTM | idem | idem | idem | Idem LSTM [H] |
| **SNN/LIF** | V[t+1]=λV+ΣI; spike se V>θ | eventos binários (0/1 no tempo) | **três eventos**: spike +, spike −, silêncio | Neuromórfico (Loihi/SpiNNaker provam eventos binários) [F] | En: semântica idêntica ao evento binário atual + 1 fio | Polaridade de pulso já é usada em alg. bi-plausíveis [X]; chave: distinguir silêncio vs zero vs falha [Q] |
| **Hodgkin-Huxley** | 4 EDOs (V,m,h,n) | contínuos em [0,1] | quantizar gates em 3 níveis | condutâncias não-lineares ~ memristores (analogia de Mead) | En/C: alto; fiel só em nível analógico | Reproduzir HH integralmente em porta lógica é desperdício [F por análise]; valor = inspiração de dinâmica de limiar [H] |
| **Capsule Nets** | routing dinâmico por acordo | vetores + normas | níveis ternários de confiança no routing | circuitos de norma/comparação | C: routing caro; ganho ternário incerto | Pouca evidência de ganho; prioridade baixa [E] |
| **KAN** | splines aprendíveis nas arestas | controle de spline contínuo | quantizar pontos de controle em 3 níveis | LUTs ternárias de transferência | C: aproximação por spline vs ReLU: trade-off aberto | Teorema de Kolmogorov-Arnold é fato [F]; KAN ternário é especulação [Q] |

**Síntese:** os modelos com pesos ternários (MLP, CNN, Transformer-weight, Perceptron-Q₃) têm
evidência experimental externa favorável ou mapeamento de circuito direto — são os primeiros
candidatos. Modelos de gates contínuos (LSTM/GRU/HH) e de normalização contínua (Transformer)
têm gargalos que a ternarização não resolve — servem de **contraexemplos** úteis para delimitar
onde a ternariedade ajuda.

---

## 3. Matriz comparativa das tecnologias físicas candidatas

Critérios do brief condensados: representação (R), escrita/leitura (W/L), retenção (Ret),
margem de ruído (Mrg), energia (En), densidade (Dens), maturidade (Mat), e — crucial —
**nível**: se a tecnologia prova memória multinível (M), lógica multinível (L) ou ambos (M+L).

| Tecnologia | R | W/L | Ret | Mrg | En | Dens | Mat | Nível | Comentário-chave |
|---|---|---|---|---|---|---|---|---|---|
| CMOS multinível (tensão) | níveis de Vdd fracionários | estático | estático com regeneração | pequena e encolhe com scaling [F] | ↑ com níveis | ↓ vs binário | protótipos | **L** | Lógica MVCM existe em pesquisa [X]; fan-out e cascateamento são o obstáculo [Q] |
| Current-mode (CML ternária) | correntes I, 2I, 3I | estático | não retenção | moderada [E] | ↑ | média | protótipos | **L** | Natural para somadores ternários analógicos; sensível a mismatch [X] |
| Memristor/RRAM | condutância multinível | pulso de tensão/corrente | não-volátil, com drift [X] | ~3-bit demonstrado, degrada | baixa | alta | fabricado comercialmente p/ memória | **M** (L em memória computacional [X]) | A melhor evidência de estados multiníveis persistentes; *não* prova ALU |
| FeFET | polarização ferroelétrica | pulso de gate | não-volátil | ~2–3 níveis demonstrados [X] | baixa | alta | pesquisa avançada | **M** | Retenção depende de ciclagem [X] |
| Neuromórfico (Loihi, SpiNNaker) | spikes binários no tempo | eventos | estático (digital) | full | muito baixa por evento | média | fabricado | **L parcial** (binário por evento) | Prova eficiência de *eventos*, não de ternariedade; adaptável a eventos de polaridade [H] |
| Osciladores/codificação temporal | frequência/fase/posição de pulso | dinâmica | dinâmica | depende de jitter [F] | baixa | — | pesquisa | **L dinâmica** | Converte tempo em valor; robustez vs jitter é o trade central |
| Resistores multiníveis discretos | T de resistor/REF | direta | estática | alta (física grosseira) | — | baixa | trivial | **M+L didática** | Útil para protótipos didáticos em bancada, não para escala |
| In-memory analog compute | MAC analógico em matriz | coluna de bitline | idem RRAM | erros multiplicativos [X] | muito baixa | alta | pesquisa | **M→L híbrido** | CAMa com pesos ternários é o cruzamento mais promissor [H] |

**Leitura honesta da matriz:** nenhuma tecnologia disponível hoje entrega "3 níveis estáveis
com margem de lógica e custo vencedor" — o que existe são (a) memórias multiníveis maduras
(RRAM/FeFET/NAND-MLC), (b) protótipos de lógica multinível frágeis, e (c) um precedente
histórico (Setun, relés, 1958, única produção em massa de computador ternário balanceado [F])
que prova a consistência arquitetural, não a competitividade física moderna.

**Custo de conversão de domínio [E, a medir]:** toda fronteira físico→digital e vice-versa
(ADC/DAC ternário) consome área/energia/latência e pode dominar o orçamento da camada 6.

---

## 4. Significados possíveis do zero — definição inicial

O zero é o elemento mais ambíguo do sistema. Enumerar seus significados é pré-requisito para
evitar ambiguidades. Cada significado deve viver em um **tipo distinto** na implementação.

| Domínio | Significado de 0 | Nota crítica |
|---|---|---|
| Aritmética (BT) | dígito neutro aditivo; identidade de + | precisa de unicidade de representação [D] |
| Multiplicação | absorvente | coerente com álgebra de inteiros [D] |
| Lógica Kleene K3 | **indeterminado** (nem T nem F) | ∧/∨/¬ são De Morgan-algebra; não há tautologias não-triviais [F matemático] |
| Lógica Łukasiewicz Ł3 | valor intermediário "possível" | implicação definida; tautologias existem (ex.: p→p) [F matemático] |
| Comparação (sign) | "igual / dentro da zona morta" | operação nativa de BT; zona morta tem largura de projeto [H] |
| SNN / eventos | **ausência de evento** | ≠ zero numérico, ≠ falha, ≠ falta de alimentação — distinção obrigatória do brief; exige sinalização explícita [Q física] |
| Elétrica tri-state | **alta impedância (Hi-Z)** | Hi-Z é desconexão, não é um valor lógico! Conflá-lo com 0 lógico é erro clássico [F de projeto] |
| Comunicação | símbolo de sincronização/"nada a transmitir" | protocolo deve distinguir de 0 de dado [Q] |
| Probabilidade | massa nula | coerente com medida; cuidado com "impossível" vs "inobservável" [Q] |
| Física de referência | ponto de repouso / referência de medição | escolha de referencia afeta margens [E] |
| Comparadores de erro | "sem erro" vs "erro desconhecido" | semântica de segurança; não colapsar [Q] |

**Decisão de desenho proposta [H]:** o estado físico {-1,0,+1} é um *portador*; as semânticas
(aritmética, Kleene, Łukasiewicz, evento, Hi-Z) são **tipos** que habitam o mesmo portador.
A pergunta científica correspondente: uma mesma célula física pode sustentar semânticas
múltiplas sem ambiguidade de interpretação? Resposta provável: sim, **se** o tipo for
transportado fora da célula (contexto de operação), nunca dentro dela [H, a formalizar].

---

## 5. Propriedades matemáticas que precisam ser provadas

Agrupadas por alvo. Todas são candidatas a prova em Lean 4/Agda e teste em Haskell.

### A. Aritmética ternária balanceada (BT)

- **A1.** Unicidade: todo n ∈ [−(3ⁿ−1)/2, (3ⁿ−1)/2] tem exatamente uma representação com n trits. [D]
- **A2.** Fechamento: soma/mult após normalização de carry ∈ {−1,0,1} retorna ao domínio. [D]
- **A3.** Associatividade e comutatividade de + e ×. [D]
- **A4.** Distributividade de × sobre +. [D]
- **A5.** Identidade e inversos: 0 neutro; d→−d é o inverso. [D]
- **A6.** Truncamento de k trits menos significativos **é** arredondamento ao mais próximo,
  sem empates (|descarte| ≤ (3ᵏ−1)/2 < 3ᵏ/2). Propriedade *notável* do BT que o binário
  complemento-de-dois não tem. [D]
- **A7.** Correção do algoritmo de conversão bin↔tern e idempotência do round-trip. [D]
- **A8.** Multiplicar por 3ⁱ = shift de i trits, sem erro. [D]
- **A9.** Monotonicidade: se a ≤ b trit a trit com ordem natural, então a+c ≤ b+c. [D]

### B. Lógicas ternárias

- **B1.** Kleene ∧/∨/¬ formam uma De Morgan-álgebra; ordem: F ≤ U ≤ T com min/max. [D]
- **B2.** Em K3, nenhuma fórmula não-trivial é tautologia (consequência de monotonicidade). [D]
- **B3.** Em Ł3, existem tautologias não-triviais (ex.: p→p); caracterizar o conjunto. [D]
- **B4.** As duas lógicas **compartilham o portador** {-1,0,+1} mas **não** as operações —
  prova de que tipos separados são obrigatórios. [D, metateorema de desenho]
- **B5.** Preservação de informação: a tabela de uma porta ternária admite inversa local
  quando e somente quando é injetiva por linha — classificar todas as portas unárias (3³=27)
  e binárias (3⁹=19683) por propriedades (de Morgan, monotônicas, conservativas). [D — espaço finito pequeno]

### C. Sistemas e hardware

- **C1.** Invariantes de máquinas de estado: todo estado alcançável é válido (de definição indutiva). [D]
- **C2.** Correção da ALU: comportamento da ALU refinado = modelo funcional de referência (comutativo com provas). [D]
- **C3.** Semântica de erro: um flip de trit sob ruído gaussiano com margem m mapeia para
  erro aritmético limitado por 3ⁱ·(peso do trit flipado) — base para ECC ternária. [D no modelo; física é empírica]
- **C4.** Comparação ternária nativa tem custo O(1) para o sinal de um número (trit mais significativo
  não nulo) — propriedade que o binário complemento-de-dois não tem. [D]

---

## 6. Experimentos estatísticos e físicos prioritários

| # | Experimento | Pergunta | Método | Critério de decisão | Custo |
|---|---|---|---|---|---|
| **E1** | Margem de ruído de leitura ternária | Que margem (fracionada em σ do ruído) mantém BER alvo? | Monte Carlo: 3 níveis + ruído gaussiano/térmico + drift; BER vs margem vs T; Cpk do processo de leitura | Curva margem×BER; comparar com binário no mesmo orçamento de sinal | baixo (simulação) |
| **E2** | Propagação de erro em cadeia de portas | Erro cresce linear ou explodidamente? | Simulação de cadeias de profundidade n com inversores/MIN/MAX ternários + ruído por estágio | Comparação com cadeia binária equivalente | baixo |
| **E3** | TRNG ternário | É possível entropia ≥ ln 3/símbolo com fonte física + extrator? | Simular fontes (térmico, avalanche, jitter de oscilador) → extrator ternário → testes NIST SP 800-90B (biblioteca) + estimador de entropia por símbolo | Min-entropia por trit medida + testes passando; distinguir PRNG/TRNG rigorosamente | baixo-médio |
| **E4** | Retenção multinível (curva de banheira) | Modelo de degradação de 3 níveis em RRAM simulado | Série temporal de drift de condutância; fit Weibull/log-normal; separar mortalidade infantil / aleatórias / desgaste | Modelo validado com IC; testar se a banheira clássica se aplica | médio |
| **E5** | Aritmética estocástica ternária | Representação probabilística de valores em trits estocásticos tem erro tolerável? | Monte Carlo de computação estocástica com dígitos aleatórios ternários; erro vs comprimento | Curva erro vs n; comparação com estocástica binária | baixo |
| **E6** | Random walk ternário (Galton ternário) | Quincunx com passos {−1,0,+1}: convergência e variância | Simulação em cascata; var por passo = 2/3 (vs binário 1/2 com passo ±½ ou 1) [D analiticamente]; testes de normalidade empírica | Convergência à normal; também possível biased-walk para gerar distribuições ternárias | baixo |
| **E7** | Matmul ternário vs binário | Energia/op estimada com pesos {−1,0,+1} | Modelo analítico de MAC aditivo + contagem de skips; varrer densidade de zeros | Curva energia vs densidade; ponto de equilíbrio | baixo |
| **E8** | Perceptron Q₃ em modelo elétrico | Comparador de 3 níveis com zona morta é robusto? | Modelo analítico de offset/ruído do comparador; ngspice se disponível (senão modelo) | Zona morta vs variabilidade de fabricação | médio |
| **E9** | Outliers: procedimento | Pipeline de investigação de anomalias | Protocolo do brief §9 aplicado a todos os experimentos acima | Toda anomalia documentada com reprodução | contínuo |

**Nota metodológica:** E1–E7 são simulações/modelos, não experimentos físicos. Nenhum
resultado deles pode ser citado como evidência de fabricação — apenas como evidência sobre
modelos. Experimentos físicos ficam para fase posterior (banheira real, protótipo em componentes discretos).

---

## 7. Estratégia de testes rigorosos, mas não exaustivos

**Espaço de estados e exaustividade honesta:** com n trits, o espaço é 3ⁿ. Exaustivo é
trivialmente factível até n = 12 (531.441 casos) e inviável a partir de ~n = 18 (387 M). A
estratégia: exaustivo para n ≤ 12, e para n > 12 combinação orientada a risco:

1. **Testes de propriedade** (QuickCheck/Hedgehog em Haskell; Hypothesis em Python): geradores
   de palavras ternárias com distribuição controlada (uniforme, valores extremos, densidade de zeros).
2. **Testes diferenciais**: toda implementação otimizada comparada contra referência com
   `Integer` de precisão arbitrária (o "modelo de ouro" matemático).
3. **Fronteiras**: ±(3ⁿ−1)/2, ±1, 0, palavras tudo-{−1}, tudo-{+1}, tudo-0, um trit flipado em cada posição.
4. **Metamórficos**: simetria de sinal (f(−x) = −f(x)), ordem de soma irrelevante, round-trip
   conversão idempotente, truncar-then-add ≤ add-then-truncate (ou igualdade, a derivar).
5. **Invariantes**: estado válido preservado por cada transição; carry sempre ∈ {−1,0,1}.
6. **Degenerados/inválidos**: entradas fora do domínio, Hi-Z tratado como desconexão (≠ 0), NaN-ternário.
7. **Estatísticos**: sementes registradas em todos os testes pseudoaleatórios; ICs reportados;
   proibido inventar percentuais de confiança.
8. **Falha simulada**: bit/trit flip injetado, ruído crescente, teste de tolerância gradual.
9. **Cada caso de teste** registra: objetivo, hipótese, método, entradas, critério, resultado
   observado, limitações, artefato reproduzível (script + seed + versões).

Proibido: declarar teste aprovado sem executá-lo; tratar propriedade testada como provada;
confundir cobertura com rigor.

---

## 8. Inventário de dependências e plano Haskell + Agda

### 8.1 Estado do ambiente (verificado em 2026-11-16)

| Dependência | Encontrada | Necessária | Ação | Motivo | Resultado/Verificação |
|---|---|---|---|---|---|
| Lean 4 + elan + lake | 4.24.0 | ≥4.x estável | nenhuma | provas formais (com skill writing-lean-proofs) | ok |
| Agda | 2.8.0.2 | ≥2.6 | nenhuma | provas formais independentes | ok |
| GHC | **ausente** | 9.x | `shell.nix` do projeto (GHC 9.10.3 via nixpkgs) | modelos de referência + QuickCheck | **Registro de tentativa anterior:** `ghcup install ghc --set` falhou (bindist Rocky-8/FHS incompatível com NixOS, configure exit 1 — log em `~/.ghcup/logs`). Solução idiomática em NixOS: `nix-shell ~/tenerura/shell.nix`. **Verificado: GHC 9.10.3 ok (exit 0)** |
| cabal | 3.18.1.0 (ghcup) | ≥3.6 | fornecido também pelo shell.nix | build do projeto Haskell | ok |
| stack | ausente | — | **não instalar** | cabal cobre; menos dependências | decisão registrada |
| Python | 3.12.15 | ≥3.10 | nenhuma | estatística/simulação | ok |
| numpy/scipy/matplotlib/pandas/sympy/statsmodels/hypothesis | **ausentes no Python do sistema** | estáveis pinned | `python3.withPackages` no `shell.nix` | **Registro de tentativa anterior:** venv + wheels do PyPI falhou no NixOS (`ImportError: libstdc++.so.6` — wheels manylinux dependem de libstdc++ que o NixOS não expõe fora do store). Venv quebrado removido; `requirements.txt` mantido para ambientes não-NixOS | **Verificado: import de todos os pacotes ok (Python 3.14.8, exit 0)** |
| Rust | toolchains em ~/.rustup (1.99.0, nightly) mas rustc/cargo fora do PATH | — | **não alterar agora**; risco registrado | não crítico nesta fase | risco: shims ausentes |
| Git | 2.56.0 | — | nenhuma | versionamento de dados/seeds | ok |

Protocolo seguido: verificou-se antes de instalar; nada global foi substituído; instalações
user-level ou dentro de venv do projeto; versões pinned em `~/tenerura/requirements.txt`.

### 8.2 Plano Haskell (modelos de referência + testes de propriedade)

Estrutura proposta (`~/tenerura/src/haskell/`):

- `Trit.hs` — `data ArithTrit = Neg | Zero | Pos`, `LogicK3`, `LogicL3` como **tipos distintos**
  sobre o mesmo portador; conversões explícitas e totais (Total Haskell).
- `BT.hs` — palavras ternárias balanceadas (`newtype BT = [ArithTrit]`), valor ⇄ palavra,
  truncamento, shift.
- `Arith.hs` — soma/mult com normalização de carry; comparação por sinal.
- `Logic.hs` — tabelas de Kleene e Łukasiewicz, min/max/implicação.
- `Gates.hs` — catálogo de portas (27 unárias, 19.683 binárias enumeráveis) com classificadores.
- `ALU.hs` — ALU ref, especificação e implementação com carry-save ternário.
- `StateM.hs` — máquinas de estados com invariantes tipados.
- `Stoch.hs` — trits estocásticos, random walk ternário, Monte Carlo com seeds registradas.
- `tests/` — propriedades A1–A9, B1–B5, metamórficos e diferenciais (§5 e §7).

Ordem: `Trit` → `BT` → `Arith` (com testes diferenciais vs Integer) → `Logic` → `Gates` → `ALU`.

### 8.3 Plano Lean/Agda (provas)

Prioridade Lean 4 (Mathlib dá inteiros, ordem, lattices de graça; skill instalada), com Agda
para formalização independente dos núcleos críticos — dois verificadores convergindo reduz
o risco de bug de prova:

- Lean/Mathlib: A1–A9, B1–B3 (De Morgan algebra no Mathlib), C3 (modelo de erro), C4.
- Agda: A1 (unicidade, indutiva), A6 (truncamento = round), C2 (refinement ALU ⇄ ref).
- Critério: "provado" = arquivo carrega sem `sorry`/`admit`, verificado por CI local.

---

## 9. Perguntas científicas ainda não resolvidas

1. **[Q]** Existe margem de ruído para lógica multinível que escale com miniaturização, ou a
   lógica multinível tem penalidade fundamentalmente superlinear com scaling? (o que faz o
   binário vencer historicamente: amplificação digital regenerativa)
2. **[Q]** Qual é o custo real (área/energia/latência) de ADC/DAC ternários nas fronteiras
   físico↔digital, e ele anula o ganho de densidade de 1.585 bits/trit?
3. **[Q]** Uma célula física única pode hospedar semânticas múltiplas (aritmética vs lógica vs
   evento) sem ambiguidade — com o tipo carregado por contexto, não pelo estado?
4. **[Q]** Hi-Z vs 0 lógico vs silêncio de evento: como o protocolo físico distingue os três
   em um único canal ternário?
5. **[Q]** A comparação ternária nativa (C4) se traduz em vantagem de pipeline mensurável?
6. **[Q]** Truncamento = round-to-nearest (A6) simplifica circuitos de arredondamento o
   suficiente para compensar a lógica ternária mais cara?
7. **[Q]** Qual fração da energia de um matmul ternário é economizada de fato com skips de
   zeros, em densidades de pesos realistas (não sintéticas)?
8. **[Q]** Existe um extrator determinístico que mapeie fonte física contínua → símbolos
   ternários com entropia por símbolo ≥ teórico menos overhead mensurável (E3)?
9. **[Q]** A degradação multinível (E4) segue banheira clássica ou regime próprio (drift
   monotônico, leptocúrtico)? O brief exige validar o modelo, não assumi-lo.
10. **[Q]** Bimodalidade em populações de dispositivos multiníveis: defeito de fabricação ou
    dois regimes operacionais? Protocolo do §9 do brief se aplica.

---

## 10. Ordem de pesquisa (risco × custo × validabilidade × descoberta)

Princípio: atacar primeiro o que é barato, formalizável e desbloqueia o resto; expor cedo as
hipóteses que, se falsas, matam ramos inteiros (efeito de menor custo de descoberta).

| Fase | Trabalho | Por quê agora | Critério de saída |
|---|---|---|---|
| **1** | Semântica formal: tipos de trit, BT, lógicas K3/Ł3 em Haskell + provas A1–A9, B1–B4 em Lean/Agda | custo ~zero, funda tudo, valida/invalida a semântica | biblioteca verde + provas sem sorry |
| **2** | E1 (margem×BER) + E2 (propagação em cadeia) — Monte Carlo | testa a hipótese central de ruído; se margens forem inviáveis, o projeto pivota para memória multinível + lógica binária | curvas margem×BER×T com IC; comparação honesta com binário |
| **3** | Gates: enumeração/classificação de todas as portas unárias e binárias (B5) + percepção Q₃ analítica (E8) | espaço pequeno e finito: exaustivo de verdade aqui; alimenta catálogo de operações | catálogo classificado + relatório de robustez do comparador |
| **4** | ALU ternária de referência + refinement provado (C2) | primeira unidade "computacional" completa; provas de correção | ALU correta por prova + testes diferenciais |
| **5** | E3 (TRNG ternário) + E5 (estocástica ternária) + E6 (Galton ternário) | aproveita a estrutura ternária; entropia e computação probabilística são usos naturais de 3 estados | entropia por trit medida; testes NIST; curvas de erro |
| **6** | E7 (matmul ternário) + E4 (retenção/degradação) | quantifica promessa de energia e de memória multinível | ponto de equilíbrio energia vs binário; modelo de retenção validado |
| **7** | ISA + microarquitetura (só com ALU provada) | evita desenhar ISA sobre fundações não validadas | especificação de ISA com semântica formal |
| **8** | Kernel/integração | último; sem valor antes das camadas de baixo | análise de integração documentada |

**Riscos que podem desviar o plano (registrados agora):** (i) E1/E2 mostrarem margens
inviáveis → pivotar para *memória ternária + aritmética híbrida*; (ii) custos de conversão
(Q2) dominarem → focar em domínios sem conversão (eventos/SNN, memória computacional);
(iii) provas de B4 revelarem ambiguidade insuperável → restringir a uma única semântica por sistema.

---

## Apêndice: fontes primárias a citar quando cada afirmação for usada

- Balanced ternary, Setun: literatura histórica (Sobolev, 1958–; Malinovsky, "Pioneers of Soviet Computing").
- BitNet b1.58: Ma et al., "The Era of 1-bit LLMs", arXiv 2402.17764 (pré-print — citar como tal).
- TWN/TWN: Li & Liu, "Ternary Weight Networks", arXiv 1605.04711.
- Lógicas K3/Ł3: Kleene (1952), Łukasiewicz (1920/1930); Handbooks de many-valued logic.
- RRAM multinível e retenção: literatura de IEDM/VLSI (a selecionar com DOI na fase E4).
- TRNG: NIST SP 800-90B; Killmann & Schindler.
- Neuromórfico: Davies et al., "Loihi" (IEEE Micro 2018); Furber, SpiNNaker.
- De Morgan álgebra / lattices: Mathlib (verificar nomes exatos ao provar).

*Documento vivo: atualizar rótulos epistêmicos conforme fases 1–3 produzirem resultados.*
