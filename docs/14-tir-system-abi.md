# TENERURA — Fase 9: TIR como alvo do sistema — arquitetura e ABI

Data: 2026-11-17 · **em cópia** (`~/tenerura-sandbox/`); `~/tenerura/`
intocado. Vizinhos: `07` (ISA — fonte de toda afirmação de máquina aqui),
`08` (núcleo), `13` (TIR), `15` (layout de memória), `16` (intrínsecas).
Toda decisão é hipótese, no formato do handoff (`12`): §8.

## 0. Relatório da fase (o que mudou, medido)

| entrega | estado | evidência |
|---|---|---|
| TIR é o caminho único do T27R | **feito**: `lang::compile` = fonte → TIR → `.tas`; o gerador de pilha virou oráculo de teste (`compile_pilha`, `t27r --pilha`) | `tests/lang.rs`, fuzzing (§0.2) |
| ABI de sistema especificada | **feito** (§2–§7) | `abi_nove_argumentos_…` |
| layout de memória determinístico | **feito** (`docs/15`) | `layout_dos_registros_do_nucleo_…` |
| intrínsecas de hardware | **feito** para o que a ISA tem; o resto recusado com motivo (`docs/16`) | 4 testes de máquina nua, 1 de privilégio |
| TIR Core: dados externos, `trap`, `sh`, limites por proveniência | **feito** (formato v2, `docs/13` §O) | `tests/tir.rs`, `tests/tir_system.rs` |
| TIR System: módulo privilegiado | **feito** (flag no cabeçalho, validação por modo) | `privilegio_e_recusado_…` |
| backend: biblioteca, rótulos globais, fusão compara-e-desvia, quadro compacto | **feito** | tabela §0.1 |
| núcleo migrando para TIR | **começou**: `irq_enable`/`irq_disable` saíram do assembly (`src/os/kernel/irq.t27r`) | toda a suíte do núcleo, §0.1 |
| fuzzing de programas | **feito**: 300 programas T27R aleatórios × 2 geradores sob o núcleo real | 0 divergências |
| Lean / Haskell estendidos | **não** (§9) | — |
| registros/arrays como tipo da linguagem, laços paralelos, otimizador | **não** (§9) | — |

### 0.1 Números (uma máquina, um dia; reexecutar antes de citar)

Núcleo, antes (`d2db51f`) → depois:

| medida | antes | depois |
|---|---:|---:|
| palavras de texto do núcleo | 1 073 | 1 236 (+15%) |
| cobertura de instruções | 99,81% (2 não executadas) | 99,19% (2 + 8 `brk` de limite) |
| IPC ida e volta | 717 + 22 | 717 + 22 |
| pilha máxima do núcleo | 24 | 24 (limite 64) |
| maior trecho não preemptível | 527 / 8 621 com reinício | 543 / 8 742 |
| `demo2` (passos) | 305 546 | 306 071 (+0,17%), usuário idêntico (8 718) |
| mutantes | 55 detectados | 55 detectados |

O preço das duas rotinas em T27R: +163 palavras e ~+17 passos por
`irq_disable(0)` (chamada a cada morte de tarefa; eram 2 instruções). As verificações de
limite são redundantes com as guardas do próprio código (não há análise de
intervalos); são os 8 `brk` nunca executados.

Código de usuário (instruções de usuário executadas, núcleo real):

| programa | pilha | TIR (Fase 8) | TIR (agora) |
|---|---:|---:|---:|
| `fib` 0..10 | 18 599 | 15 389 | 13 291 (−29%) |
| `pingr` | 953 | 741 | 646 (−32%) |
| laço soma | 382 | 317 | 263 |
| fatorial + `match` | 317 | 274 | 253 |
| precedência e lógica | 211 | 207 | 182 |
| 300 programas aleatórios (soma) | 402 772 | — | 386 894 (−4%) |

Compilação (`tir_bench 2000 7`): lower 6,2 ms, validação 4,0 ms,
TIR → `.tas` 22,0 ms (1 thread) / 4,5 ms (56); gerador de pilha 19,5 ms.
Sequencial o caminho TIR continua mais lento (32 × 19,5 ms); paralelo,
mais rápido (13,6 ms).

### 0.2 Testes novos

`tests/tir_system.rs` (9): máquina nua com camada 0 + TIR (MMIO, CSR,
`csrx`, `tlbi`, `wfi` com IE = 0), índice fora → `brk` sem escrever,
conformidade da ABI (9 argumentos, s1..s11 e sp preservados), privilégio
nos dois sentidos (linguagem e TIR), proveniência estática, `irq.gen.tas`
em dia com a fonte e com o `.data` do núcleo, layout de TCB/capacidade/
entrada de boot contra a regra do `docs/15`, constantes Rust × `abi.inc`,
e fuzzing diferencial (300 programas, semente fixa, I1–I12 a cada saída
do núcleo). Suíte inteira: §0.3.

### 0.3 Estado verificado

`tools/verificar-tudo.sh` no fim da fase (2026-11-17): **100 testes, 0
falhas** (91 antes + 9 de `tir_system`); Rust × Haskell 30 000 casos, **0
divergências**; provas Lean ok (as mesmas 46 — nada novo provado).

## 1. Arquitetura em camadas — o que existe em cada uma

```
5  linguagens       T27R → TIR (único caminho). Outras: contrato = §2 + docs/13.
4  núcleo/runtime   micronúcleo em assembly + irq.t27r (TIR). Migração por rotina.
3  backend          irgen: TIR → .tas (programa ou biblioteca), paralelo, determinístico
2  TIR System       módulo privilegiado + @tir.intr.{csr_*, phys_*, wfi, tlbi}
1  TIR Core         word/ptr, regiões, global/slot/ptradd/load/store, trap
0  assembly mínimo  boot, entrada/saída de trap, troca de contexto (eret)
```

A camada 0 **não encolhe a zero**, e não por falta de trabalho: entrada de
trap, troca de contexto e `eret` manipulam *todos* os registradores
(inclusive `sp` e `lr` de outro contexto) e a pilha que o próprio código
usaria. Nenhuma função com ABI consegue expressar isso (§6). O que pode
sair da camada 0 é tudo o que roda *depois* de o contexto estar salvo: o
tratamento dos eventos. Hoje: 2 rotinas.

## 2. Convenção de chamada (código TIR ↔ código TIR ↔ assembly)

Registradores (docs/07 §3, sem invenção): `zr`; `a1..a9` (+1..+9);
`t1..t4` (+10..+13); `s1..s11` (−1..−11); `lr` (−12); `sp` (−13).

| item | regra |
|---|---|
| argumentos | `a1..a9`, em ordem; **no máximo 9** (validador do backend recusa mais) |
| resultado | `a1` (funções têm 0 ou 1 resultado; T27R sempre 1) |
| quem chama preserva | `a1..a9`, `t1..t4`, `lr` |
| quem é chamado preserva | `s1..s11`, `sp` |
| chamada / retorno | `call fn_<nome>` (= `jal lr, rel`) / `ret` (= `jalr zr, 0(lr)`) |
| o que o código TIR usa | `a1..a9` (só como argumentos), `t1..t3`, `s11` (quadro), `lr`, `sp`. **Nunca** `t4` nem `s1..s10` |
| valores de 27 trits | uma palavra por argumento/resultado; `ptr` também é uma palavra (endereço) |
| ordem de avaliação | argumentos da esquerda para a direita (docs/13 §E) |

`t4` e `s1..s10` livres de propósito: o núcleo usa `t4` na entrada de
trap e `s1` como TCB corrente; uma função TIR chamada de dentro de um
tratador não toca nisso.

## 3. Quadro de pilha

```
entrada:  push lr ; push s11 ; mov s11, sp ; addi sp, sp, -F
valor k:  -(k+1)(s11)          k ∈ [0, nvalores guardados)
slot:     -(base+n)(s11) .. -(base+1)(s11)   (palavra 0 no endereço mais baixo)
saída:    .Lret_<f>: mov sp, s11 ; pop s11 ; pop lr ; ret
```

- A pilha cresce para endereços menores; `push` = `addi sp,sp,-1; st`.
- **Alinhamento: 1 palavra.** A memória é endereçada por palavra
  (docs/07 §6); não existe acesso a subpalavra, logo não existe
  desalinhamento.
- F = valores guardados (constantes, comparações fundidas e argumentos de
  laço não ocupam palavra) + áreas de `slot`. Uso de pilha por quadro ativo:
  F + 2. `irq_enable`/`irq_disable`: 16.
- `sp` na entrada pode ser qualquer palavra; não há zona vermelha.

## 4. Símbolos e compatibilidade entre módulos

| entidade | símbolo no `.tas` |
|---|---|
| função TIR `f` | `fn_f` |
| dado externo (`global`) `g` | `g` — definido por quem inclui (ex.: `.data` do núcleo) |
| rótulos internos | `.L_<f>_<n>`, `.Lret_<f>`, `.Ltrap_<f>_<n>` |

Rótulos internos levam o nome da função, logo dois módulos TIR (ou TIR e
assembly) cabem no mesmo `.tas` desde que os nomes de função sejam únicos
na imagem — a mesma regra que `fn_<f>` já impõe. A ligação é textual
(`.include` do montador); não há formato de objeto relocável. Módulo com
entrada vira programa (`_start: call fn_main; mov a2, a1; j exit` +
`ulib`); sem entrada (`NO_ENTRY`), biblioteca só com as funções.

Compatibilidade de dados entre módulos: o módulo declara tamanho
(`static mut x: [word; n]`); quem define o símbolo garante pelo menos n
palavras. Conferido para o núcleo por teste (o próximo rótulo do `.data`
está a ≥ n palavras).

## 5. Camada 0 mínima (boot)

Contrato que o código TIR do núcleo espera ao ser chamado:

1. modo núcleo (endereço físico, docs/07 §6), `IE = 0`;
2. `sp` aponta para pilha válida com espaço para a cadeia de quadros
   (o núcleo usa `li sp, 0` e confere ≤ 64 palavras, `KSTACK_LIMIT`);
3. `tvec` aponta para um tratador (senão um `trap` do TIR salta para 0).

Exemplo completo e testado (`tests/tir_system.rs`, `BOOT0`): 8 instruções
— `li sp, 0`, `csrw tvec`, `call fn_kmain`, desligar com `a1`; tratador
que desliga com a causa. É a transição camada 0 → TIR em sua forma mínima.

## 6. Traps, interrupções, chamadas de sistema e troca de contexto

Tudo aqui é **camada 0** e continua em `kernel.tas`; a ABI só diz o que o
código TIR pode assumir.

- **Entrada de trap** (`trap_entry`): `csrx t4, scratch, t4` troca o `t4`
  do usuário pelo TCB corrente; salva `a1..t4` em `+1..+13(TCB)` e
  `s1..sp` em `−1..−13(TCB)`, `epc` em `0(TCB)` (docs/15 §4); `s1 ← TCB`,
  `sp ← 0`; despacha pelo **sinal** de `cause` (−: falha, 0: `sys`,
  +: interrupção).
- **Saída** (`resume_task`): restaura na ordem inversa, `status ←
  ST_USER_RET`, `eret`.
- **Chamada de sistema** (usuário, `abi.inc`): número em `a1`, argumentos
  em `a2..a9`, paciência em `t1`, `sys`; volta com `a1` ∈ {+1, 0, −1} e
  dados em `a2..a9`/`t1`. As intrínsecas de usuário `call`, `recv`,
  `recv_val`, `yield`, `time` (docs/16 §2) seguem exatamente isto.
- **Código TIR dentro do núcleo** roda só dentro de um tratador: depois de
  salvo o contexto, com `IE = 0`, núcleo não preemptível e pilha única.
  Não há reentrância a tratar. Escrever `IE = 1` por `csr_write(1, …)` é
  permitido pela ISA e pelo TIR, mas **quebra o contrato do núcleo**; o
  validador não tem como saber o valor escrito.
- **Troca de contexto** = escrever outro TCB em `s1` e ir para
  `resume_task`. A parte que escolhe o TCB (escalonador) é expressável em
  TIR; a que restaura registradores e faz `eret` não é (§1).

## 7. Determinismo

Mesmo fonte → mesmos bytes de TIR → mesmo `.tas`, com 1 ou N threads
(testado: `paralelo_e_sequencial_…`, `modulo_do_nucleo_em_dia_…`, que
compara o `.gen.tas` versionado com o gerado em 1 e 8 threads). Nenhuma
saída depende de `HashMap`, de endereço ou de escalonamento.

## 8. Decisões como hipóteses

**S1. Migrar o núcleo por rotina, não reescrever.**
Por que: cada passo mantém I1–I12, os 55 mutantes, caos e exaustão
verdes. Falsificação: se o custo por rotina (aqui +15% de texto por 2
rotinas, ~5× por instrução no laço quente que *não* foi migrado,
`zero_frame`) não cair com o otimizador, migrar caminhos quentes piora o
núcleo. Confiança: **alta** para o método, **baixa** para migrar
caminhos quentes antes de alocação de registradores.

**S2. Quadro em memória, sem alocador de registradores.**
Por que: correção e um único caminho; a fusão compara-e-desvia já tira
29–32% das instruções. Falsificação: `zero_frame` em T27R gastaria ~15
instruções por palavra contra 3 do assembly. Confiança: **média** — é o
próximo gargalo, conhecido.

**S3. `t4` e `s1..s10` fora do alcance do código gerado.**
Por que: deixa o TIR chamável de qualquer ponto do núcleo sem salvar
nada. Falsificação: um alocador de registradores vai querer `s*`; aí ele
salva o que usar, como manda §2. Confiança: **alta**.

**S4. Ligação textual (`.include`), sem objeto relocável.**
Por que: é o que o montador tem; a imagem é pequena. Falsificação:
primeira biblioteca TIR usada por dois programas com nomes que colidem.
Confiança: **média**.

**S5. Erro de compilação para índice constante fora do array.**
Por que: o TIR proíbe acesso com deslocamento constante fora da alocação
(validador, docs/13 §O); o `rustc` faz o mesmo (`unconditional_panic`).
Achado: um teste de máquina nua mostrou que `let i = 5; buf[i]` chega ao
TIR como constante pelo SSA, e o validador recusava o módulo. Confiança: **alta**.

## 9. O que não foi feito

- **Lean e Haskell não foram estendidos.** As provas cobrem a ALU; nada do
  TIR (semântica, validação, layout) é provado. O diferencial Rust ×
  Haskell segue cobrindo a ISA de usuário; o TIR tem o seu diferencial
  (dois geradores + fuzzing), não uma segunda implementação da semântica.
- **Interpretador de referência do TIR**: continua faltando.
- **Chamar assembly a partir do TIR** (`extern fn`): não há sintaxe.
- **Registros e arrays como tipos de valor/local** na linguagem: só
  `static` de array/escalar (docs/15 §6).
- **Paralelismo em execução** (laços paralelos, atômicas além de `csrx`):
  máquina de um núcleo, sem modelo de memória concorrente (docs/07 §8);
  ver docs/16 §4.
- **Otimizações** além da fusão compara-e-desvia: nenhuma (dobra de
  constantes, DCE, intervalos para remover verificações redundantes,
  alocação de registradores).
- **Autohospedagem**: o compilador é Rust no hospedeiro; não há caminho
  para rodá-lo na máquina T27 nesta fase.

## 10. Adendo 2026-11-17 (Fase 10)

Nota aditiva a §3.
- A palavra de cada valor agora é dada **por escopo**: valores de regiões
  irmãs dividem palavras, e F = o caminho mais fundo da árvore de regiões
  + áreas de `slot`.
- O formato do quadro (entrada, `-(k+1)(s11)`, saída) não mudou.
- S2 foi falsificada em parte: com uma palavra por valor, um `main` grande
  estourava a pilha de 729 palavras (`docs/18` §18.2).
- A ABI v2 projetada (resultados múltiplos, enums em registradores) está em
  `docs/18` §11.2; ainda não está implementada.
