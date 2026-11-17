# 19 — Avaliação: ILP, VLIW, DLP e alocação de registradores

Data: 2026-11-17 · modo: **só pesquisa e medição** (nenhum fonte de produção
alterado; experimentos em cópia de rascunho) · evidência: `experiments/fase11-paralelismo/`
(harness `pesquisa.rs.txt`, log `pesquisa-2026-11-17.log`, `zero_frame-x27.patch`).

Rótulos: **[M]** medido no emulador · **[P]** projeção sobre traço medido (modelo
declarado) · **[E]** fato da especificação/código, conferido.

## 0. Resposta curta

1. Na máquina de hoje, **passo = instrução** **[E]**. Reordenar não muda
   passo nenhum; agrupar (VLIW) exige mudar a ISA e redefinir o passo. O que
   reduz passos é **executar menos instruções**.
2. No código de usuário gerado pelo TIR, **30% das instruções executadas
   são tráfego de quadro** (`ld`/`st` em `s11`) **[M]**. Nos programas de
   cálculo, 29–59%. Uma alocação de registradores ideal tira **−30%** do
   corpus inteiro e **−49%** dos programas de cálculo **[P]**. Nenhuma outra
   ideia avaliada chega perto, por custo ou por ganho.
3. O paralelismo de instrução disponível é pequeno e se esgota em
   **largura 3**: ILP∞ = 1,75 depois da alocação ideal; largura 3 dá 1,65×;
   4 dá 1,75×; 9, 16 e 27 não dão mais nada **[P]**. Bloco dinâmico médio:
   6,0 instruções (usuário), 3,9 (núcleo) **[M]**.
4. DLP **já existe sem mudar a ISA**: em ternário balanceado, 3 lanes de
   9 trits com sinal somam com um `add` comum, sem máscara nem trit de
   guarda; extrair é `shi`. Laço de soma de vetores: **2,99× menos passos**,
   resultado conferido **[M]**. Aplicação estreita (só assembly/núcleo hoje).
5. Em cargas de sistema o **núcleo domina**: no `pingr`, 98% dos passos são
   do núcleo, e 70% deles são **boot** — `zero_frame` sozinho é 55% **[M]**.
   Desenrolar esse laço ×27 (729 = 27², sem resto) tira **−34%** dos passos
   totais do `pingr` **[M, em cópia]**. Nenhuma otimização de compilador
   alcança isso nesta carga.

## 1. O que a máquina realmente oferece (conferido contra docs/07 e o emulador)

| item | fato | fonte |
|---|---|---|
| modelo de execução | `Machine::step` executa **uma** instrução por passo (`time += 1`); interrupção pendente consome o passo sem executar; não há latência, pipeline nem unidade funcional modelada | `machine.rs::step` **[E]** |
| custo | métrica disponível = instruções executadas / passos; tempo de hospedeiro não é métrica da arquitetura | `Stats` **[E]** |
| codificação | 1 instrução = 1 palavra de 27 trits; formato R usa 14 trits (op 5 + 3 registradores × 3), 13 ficam zero; uma 2ª operação de 3 registradores precisaria de 14 → **não cabe** (28 > 27) | docs/07 §3 **[E]** |
| registradores | 27: `zr`, `a1–a9`, `t1–t4` voláteis; `s1–s11` preservados; `lr`, `sp`. **24 alocáveis**; o gerador TIR usa `s11` como ponteiro de quadro | docs/07 §3 **[E]** |
| imediatos | só `addi` (16 trits), `shi`, `lui`, deslocamentos de `ld/st`; `mul`, `cmp` e desvios são registrador-registrador | docs/07 §4 **[E]** |
| desvios | comparam **dois registradores** direto (`blt a,b`…); `cmp` entrega um trit | docs/07 §4 **[E]** |
| potência de 3 | `shi k` = ×3ᵏ; `shi −k` = divisão centrada por 3ᵏ, **idêntica** a `div` (provado: `div_pow3_drops_low_digits`) | docs/07 §4.1–4.2 **[E]** |
| paralelismo de dados | nenhuma instrução por lane; `kmin`/`kmax`/negação são trit a trit (27 lanes de 1 trit, nativas); soma/sub/escala por constante funcionam por lane em palavras empacotadas (§4) | docs/07 §4 + **[M]** |
| memória no T27R | modo usuário **não tem memória** (sem arrays); `static mut` e ponteiros só em `--kernel` | AI.md / docs/18 **[E]** |

Consequência para as hipóteses: a vantagem do TIR para análise de alias
(proveniência exata, ponteiros opacos) **não tem sobre o que agir no código
de usuário**: 100% dos acessos de memória medidos no usuário são de quadro
ou de pilha (`s11`/`sp`) **[M]** — tráfego que existe porque não há
alocação de registradores.

## 2. Método

- **Corpus**: 8 programas de cálculo escritos para isto (`fib` recursivo
  com impressão, `fib` iterativo, `gcd`, Collatz, primos, soma de trits,
  Horner, laços aninhados com `match`); as 10 sondas compiláveis da
  verificação (p1..p11); os **300 programas** do fuzzer diferencial
  (`Fz`, sementes `0x7151_0000+1..300`); o sistema `pingr` (T27R) ×
  `pong` (assembly). Cada um roda sob o **núcleo real**, nos dois geradores
  (`pilha` e `tir`).
- **Traço**: cada instrução executada com modo, instrução decodificada e
  endereço efetivo de `ld`/`st`.
- **Blocos**: sequência dinâmica até um desvio/salto/trap ou troca de modo.
- **Escalonamento** (projeção **[P]**): por bloco, DAG com dependências
  verdadeiras de registrador e de memória (desambiguação **perfeita** por
  endereço); variantes *real* (com WAR/WAW de registrador), *renomeado* e
  *promovido* (tráfego de quadro removido e reescrito como dependência de
  registrador = **alocação ideal**, sem derrame nem salvamento). Latência
  unitária; desvio no último ciclo; escalonamento em lista por altura,
  larguras 1, 2, 3, 4, 9, 27, ∞. É **limite superior** do escalonamento
  estático dentro do bloco (sem especulação nem movimento entre blocos).
- **Peepholes**: padrões adjacentes no traço (limites inferiores).

## 3. Resultados

### 3.1 Onde estão os passos (núcleo × usuário) [M]

| carga | usuário | núcleo | % usuário |
|---|---:|---:|---:|
| 8 programas de cálculo (TIR) | 822 954 | 126 252 | 48–92% cada (o menor é o `fib` que imprime) |
| 10 sondas (TIR) | 1 415 | 83 591 | 1,7% |
| 300 programas do fuzzer (TIR) | 848 222 | 10 321 361 | 7,6% |
| `pingr` + `pong` (TIR) | 695 | 31 915 | 2,1% |

Perfil do núcleo no `pingr` (por rótulo): **boot = 22 239 (70%)**, com
`zero_frame` 17 520 (55%) e `load_segment` 3 515 (11%). Depois do boot,
≈ 197 instruções de núcleo por trap (49 traps), das quais `trap_entry` 38,
`resume_task` 38 e `schedule` 38.

### 3.2 Mix do código de usuário (TIR, corpus inteiro, 1 672 591 instr.) [M]

quadro (`s11`) **29,8%** · `li` 12,7% · ALU 10,7% · ajuste de `sp` 8,6% ·
pilha (`sp`) 8,3% · salto/chamada 7,4% · desvio 7,4% · mul/div 5,8% ·
`mov` 4,1% · `cmp` 3,3%. Acessos de quadro por índice de slot: ≤ 4 →
53%, **≤ 10 → 91%**, ≤ 24 → 98%.

O gerador TIR emite, para cada operação, carrega-operandos / opera /
guarda-resultado, e as cópias de `yield` passam pela memória
(`ld t1,-5(s11); st t1,-1(s11)`).

### 3.3 Alocação de registradores (projeção pela promoção do quadro) [P]

| programa (TIR) | instr. hoje | com RA ideal | Δ |
|---|---:|---:|---:|
| `fib` recursivo | 13 315 | 9 441 | −29% |
| `fib` iterativo | 38 315 | 23 307 | −39% |
| `gcd` | 205 983 | 119 113 | −42% |
| Collatz | 251 820 | 125 240 | −50% |
| primos | 81 649 | 39 704 | −51% |
| soma de trits | 156 530 | 67 784 | −57% |
| Horner | 41 531 | 20 525 | −51% |
| laços + `match` | 33 811 | 13 825 | −59% |
| **8 de cálculo** | **822 954** | **418 939** | **−49%** |
| **corpus inteiro** | **1 672 591** | **1 173 894** | **−30%** |

Custo que a promoção ideal ignora: salvar registradores preservados. Pondo
os slots 1..10 em `s1–s10`, cada chamada paga 2·min(quadro, 10): 77 906
instruções no corpus, contra 454 mil acessos de quadro removidos (líquido
≈ −22%). No `fib` recursivo o salvamento (5 442) **supera** os acessos que
somem (3 874): valores que não atravessam chamada têm de ir para
registradores **voláteis**. Isso define o alocador mínimo útil (§5).

### 3.4 Peepholes baratos (TIR, corpus inteiro) [M, limites inferiores]

| padrão | ocorrências | % das instr. | troca |
|---|---:|---:|---|
| `cmp t,x,y` + desvio em `t` vs `zr` | 45 583 | 2,7% (4–7% nos laços) | desvio direto `blt/bge… x,y` (−1) |
| `li c` + `add`/`sub` com `c` | ≈ 35 000 | 2,1% | `addi` (−1) |
| `li 0` | ≈ 35 000 | 2,1% | usar `zr` (sobrepõe-se à linha anterior) |
| carga de slot já em registrador no bloco | 20 612 | 1,2% | some com RA |
| `li 3ᵏ` + `mul`/`div` | ≥ 6 124 | 0,4% (3,6% na soma de trits) | `shi ±k` (−1; exato por docs/07 §4.1) |

### 3.5 ILP e emissão múltipla [P]

Ciclos com largura W (usuário, TIR, corpus inteiro; base = 1 672 591):

| variante | W=2 | W=3 | W=4 | W=9 | W=16/27/∞ |
|---|---:|---:|---:|---:|---:|
| real (WAR/WAW) | 1,50× | 1,61× | 1,66× | 1,66× | 1,66× |
| renomeado | 1,58× | 1,71× | 1,77× | 1,77× | 1,77× |
| **depois da RA ideal** (sobre 1 173 894) | 1,52× | **1,65×** | 1,75× | 1,75× | 1,75× |
| núcleo (assembly à mão, 10,5 M) | 1,60× | 1,77× | 1,84× | 1,93× | 1,94× |

Slots vazios (NOP) com pacote fixo de W, depois da RA: W=3 **45%**, W=4
56%, W=9 81%, **W=16 89%**. O núcleo, que já é código com registradores
alocados à mão, tem o mesmo teto (≈ 1,9×): é a referência realista do que
sobra de ILP depois da RA.

### 3.6 DLP por SWAR ternário [M]

Palavra = v₀ + v₁·3⁹ + v₂·3¹⁸, cada vᵢ com sinal em ±9 841. Como a
representação balanceada é simétrica e única, a soma das palavras **é** a
concatenação das somas por lane quando cada resultado cabe em 9 trits: sem
máscara, sem trit de guarda, sem tratamento de sinal (em binário, o
complemento de dois propaga empréstimo entre lanes negativas). Extração:
`hi = shi w,−9`; `v₀ = w − shi(hi, 9)`; `v₂ = shi hi,−9` (arredondamento ao
mais próximo, docs/07 §4.2) — 6 instruções para 3 lanes, conferido com
extremos −4 321, 9 840, −9 841.

| C[i] = A[i] + B[i], 729 elementos (aleatórios com sinal) | passos |
|---|---:|
| escalar | 6 571 |
| empacotado 3×9 trits (243 palavras) | 2 197 (**2,99×**) |

Vale para `add`/`sub`/negação/escala por constante; **não** vale para
comparação, mínimo/máximo numérico ou desvio por lane. `kmin`/`kmax` são
SIMD nativo de 27 lanes de 1 trit (lógica de Kleene). Agrupamentos 9×3
trits (lanes ±13) seguem a mesma regra.

### 3.7 Boot: o maior ganho em passos totais [M, em cópia]

`zero_frame` faz `st; addi; ble` por palavra (3 instr./palavra × 729).
Desenrolado ×27 (27 `st` + `addi` + `ble` por 27 palavras, **sem laço de
resto porque 729 = 3⁶**):

| `pingr` | antes | depois |
|---|---:|---:|
| `zero_frame` | 17 520 | 6 288 (−64%) |
| núcleo | 31 915 | 20 683 |
| **passos totais** | **32 610** | **21 378 (−34%)** |

Suíte contra o núcleo alterado: 110/111; a falha é o mutante textual
"quadros não são zerados" de `tests/mutants.rs`, cujo trecho-alvo deixa de
existir (precisa de mutante novo — remover todos os `st` do laço). Nenhum
teste de comportamento (invariantes I1–I12, cobertura, exaustivo) falhou.

## 4. Análise comparativa

| alternativa | ganho em passos **hoje** | ganho se houver hardware multi-emissão | muda ISA? | custo de implementação | veredito |
|---|---|---|---|---|---|
| **RA** (slots → registradores, voláteis × preservados) | −22% a −30% corpus; −49% cálculo | multiplica com ILP | não | médio (irgen: vida por região, 24 regs, derrame) | **fazer primeiro no compilador** |
| peepholes (`cmp`→desvio, `addi`, `zr`, `shi` 3ᵏ) | ≈ −5% a −7% | — | não | baixo (irgen local) | fazer junto/antes da RA |
| desenrolar laços de boot (`zero_frame`, `load_segment`) | −34% em `pingr` | — | não | muito baixo (núcleo) + mutante novo | **maior ganho por linha** |
| caminho de trap (salvamento parcial em `sys`) | **não medido**: salvar+restaurar ≈ 76 das ≈ 197 instr./trap; o que sobra depende de quais registradores o núcleo usa | — | não (mas muda contrato do núcleo) | médio, exige decisão de ABI | medir antes de decidir |
| DLP SWAR 3×9 | 3× em laços de dados empacotados | — | não | baixo como biblioteca/intrínseca | sob demanda (não há carga hoje) |
| escalonamento por DAG (ILP) | **0** (passo = instrução) | ≤ 1,75× com W ≥ 4; 1,65× com W=3 | não para escalonar; sim para lucrar | médio-alto | adiar até existir modelo de custo |
| VLIW / pacotes estáticos | **0**, e piora densidade | igual ao anterior, com 45% (W=3) a 89% (W=16) de NOPs | **sim** (formato de pacote, unidades, emulador) | alto | não recomendado agora |

Respostas diretas às perguntas:

- **DAG por região usando ponteiros sem alias e regiões sem PHI**: ganho
  real hoje = 0 passos. O DAG em si é barato no TIR (SSA, escopo léxico,
  proveniência exata), mas no usuário não há memória além do quadro, e a
  máquina é escalar. Útil mais tarde como base de *list scheduling* se
  surgir hardware com latências.
- **Reordenar reduz pressão de registradores, derrames ou passos?** Passos:
  não (contagem invariante). Antes da RA, adiantar operações alonga vidas e
  **aumenta** a pressão; depois da RA não muda a contagem. Na geração atual
  não há derrame a reduzir: tudo já está na memória.
- **Pacotes fixos de 16** são subótimos porque o paralelismo disponível
  acaba em ≈ 4 operações por ciclo e o bloco médio tem 4–6 instruções: 89%
  dos slots seriam NOP. **Largura 3** é o joelho medido (94% do ILP∞
  depois da RA) — a coincidência com 3 é empírica, não mística: deriva do
  tamanho de bloco, não da base.
- **Pipeline estático completo** (DAG + modelo de latência/vazão + alocador
  de recursos + validador + fallback dinâmico): sem hardware, o modelo de
  latência seria inventado; o retorno medido é zero na métrica atual.
- **DLP**: não há instrução por lane; mas a aritmética balanceada torna
  SWAR de soma/escala **correto sem suporte** — é extensão de biblioteca,
  não de ISA. Comparações por lane exigiriam ISA nova.

## 5. Arquitetura recomendada e ordem

1. **Núcleo, boot** (decisão do usuário — muda `kernel.tas`): desenrolar
   `zero_frame` ×27 e atualizar o mutante; depois medir `load_segment`.
   −34% de passos no `pingr`, risco mínimo.
2. **irgen: peepholes locais** — desvio direto em vez de `cmp`+desvio,
   `addi` para constante de 16 trits, `zr` para zero, `shi` para ×3ᵏ e
   ÷3ᵏ. Pequenos, independentes, testáveis com o fuzzer diferencial que já
   existe (pilha × TIR).
3. **irgen: alocação de registradores** sobre o TIR, sem infraestrutura
   nova de liveness: valores em ordem linear do TIR, vida = [definição,
   último uso], estendida até o fim do laço quando usada dentro de `while`
   (regiões estruturadas tornam isso uma passada só). Classe do
   registrador pela travessia de chamada: valor que não cruza `call` →
   `t`/`a` livres; que cruza → `s1–s10`; resto → slot de quadro como hoje.
   Varredura linear basta (91% dos acessos usam ≤ 10 slots). Critério de
   aceitação: fuzzer diferencial 0 divergências, `irq.gen.tas` dentro do
   limite I9, e as instruções do §3.3 medidas, não projetadas.
4. **Núcleo, caminho de trap**: medir o salvamento parcial em `sys`
   (voláteis não precisam ser preservados numa chamada voluntária) contra o
   contrato do docs/14.
5. **DLP**: tipo empacotado 3×9 como biblioteca/intrínseca do T27R
   `--kernel` quando houver uma carga de dados que o justifique.
6. **ILP/VLIW**: só depois de existir um modelo de custo de hardware que a
   métrica do projeto adote. Se vier, emissão de largura 3 sobre o código
   já alocado, escalonada por lista sobre o DAG do TIR — não pacotes fixos.

Pipeline ternário proposto para o TIR: `lower` → seleção de instruções com
idiomas ternários (desvio direto de dois registradores, `shi` para 3ᵏ,
`match` de trit como dois desvios sobre o sinal sem `cmp`, Kleene em
`kmin`/`kmax`) → varredura linear por região → peephole → `.tas`.

## 6. Limites desta avaliação

- Escalonamento: latência unitária e desambiguação perfeita = limite
  superior; sem especulação nem movimento entre blocos.
- RA "ideal" = sem derrame nem salvamento; o custo de salvamento é estimado
  à parte (§3.3). O ganho real fica entre −22% e −30% no corpus.
- O corpus de usuário não tem memória (limite do T27R de usuário); o DLP foi
  medido só em assembly na máquina nua.
- Uma máquina, determinística; passos = instruções; tempo de hospedeiro não
  medido. Peepholes contados por adjacência no traço (limites inferiores).
- O desenrolar do boot foi medido em **cópia**; produção não mudou.
