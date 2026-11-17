# TENERURA — Fase 9: layout de memória determinístico

Data: 2026-11-17. Vizinhos: `07` §6 (memória da máquina), `08` (núcleo),
`13` (TIR), `14` (ABI). Teste: `layout_dos_registros_do_nucleo_segue_a_
regra_de_docs15` em `tests/tir_system.rs`.

## 1. Unidade

A unidade de armazenamento e de endereço é a **palavra de 27 trits**.
Não há byte, tryte endereçável nem acesso a subpalavra (docs/07 §6).
Consequências que simplificam tudo o que segue:

- todo campo escalar ocupa exatamente 1 palavra;
- **alinhamento é sempre 1**; não há preenchimento (*padding*) em lugar
  nenhum, então o layout é função só da ordem e dos tamanhos;
- `ptr` ocupa 1 palavra (o endereço), mas é opaco no TIR (docs/13 §C).

## 2. Registro

Um registro é uma lista **ordenada** de campos `(nome, tamanho)`:

```
desloc(campo_i) = base + Σ_{j<i} tamanho(campo_j)
tamanho(registro) = Σ tamanho(campo_j)
```

- **Sem reordenação**, nunca: a ordem de declaração é o layout.
- **`base` = 0** (registro normal: ponteiro na primeira palavra) **ou
  −(tamanho−1)/2** (registro **centrado**: ponteiro no meio, tamanho
  ímpar). O centrado é nativo do ternário balanceado: deslocamentos de
  −k a +k cabem no mesmo imediato, e o centro é a palavra 0.
- Registro aninhado: o campo tem o tamanho do registro interno e o mesmo
  layout dentro dele (deslocamentos internos somados ao do campo).

## 3. Array

`[T; n]`: n elementos contíguos, sem preenchimento; elemento i em
`base + i·tamanho(T)`. Índice válido: 0 ≤ i < n. No T27R o acesso é
verificado (docs/13 §O): índice constante fora é erro de compilação;
índice de execução fora para em `trap` (`brk`, causa −6) **antes** do
acesso.

## 4. Estruturas do núcleo, como registros (conferidas por teste)

O núcleo define os deslocamentos com `.equ`; o Rust (construtor de imagem
e `kcheck`) lê os mesmos símbolos do montador, nunca cópias. O teste
reconstrói cada layout pela regra de §2 e compara com os símbolos.

**TCB** — registro centrado de 81 palavras (`TCB_SIZE`, ponteiro no centro):

| campo | tamanho | deslocamento |
|---|---:|---:|
| `T_CAPS` (9 capacidades) | 27 | −40 |
| registros `s1..sp` salvos (reg. r em r) | 13 | −13 |
| `T_PC` (o lugar do `zr`, que não se salva) | 1 | 0 |
| registros `a1..t4` salvos (reg. r em r) | 13 | +1 |
| `T_STATE` … `T_GEN` (20 escalares, na ordem do `.equ`) | 20 | +14 … +33 |
| livre | 7 | +34 … +40 |

A área de registradores é indexada **pelo número do registrador** (docs/07
§3: −13..+13). É o que torna a entrada de trap uma sequência fixa de `st`
(docs/14 §6), e é a "estrutura de contexto de execução" do sistema.

**Capacidade** — registro de 3: `CAP_KIND` 0, `CAP_OBJ` 1, `CAP_BADGE` 2.
`CAP_SLOTS` = 9 delas ocupam `T_CAPS` (27 palavras).

**Entrada da tabela de boot** — registro de 42 (`BE_SIZE`): 15 escalares
`BE_TEXT_PHYS` … `BE_DEV_VPN` (0..14) e `BE_CAPS` (27, a partir de 15).

**Descritor de página (PTE)** — 1 palavra, campos por trit (docs/07 §6):
centro do quadro + flags nos 6 trits baixos (trit 0: +1 folha, −1 tabela;
trit 1: gravável; trit 2: executável; 3–5 zero). Não é registro de
palavras: é uma palavra com campos de trit, montada por aritmética.

**Dispositivos** — uma página por dispositivo (docs/07 §7): console em
3¹⁵ (+0 TX, +1 RX, +2 STATUS), energia em 3¹⁵+729. Acesso só por
`phys_read`/`phys_write` (docs/16), nunca por `ptr`.

**Argumentos de chamada de sistema** — **não existem em memória**: tudo
trafega em registradores (`abi.inc`); o núcleo nunca segue ponteiro de
usuário. É uma decisão do núcleo (docs/08), e a mais importante para
layout: não há estrutura compartilhada usuário↔núcleo para versionar.

**Metadados de alocação do núcleo**: alocador de quadros por avanço
(`free_next`, 1 palavra) e `irq_on: [word; 28]`, `imask_shadow: word` —
estes dois declarados também em T27R (`src/os/kernel/irq.t27r`).

## 5. Compatibilidade entre módulos

Para dados compartilhados entre assembly, Rust e TIR, a fonte única é o
símbolo do montador. TIR declara (`static mut x: [word; n]`) e a
validação de ligação confere que o objeto definido tem ≥ n palavras
(docs/14 §4). Um campo novo no TCB muda só o `.equ`; quem lê pelo símbolo
continua certo; o teste de layout acusa se a regra de §2 for violada
(campo sobreposto ou fora de ordem).

## 6. O que não foi feito

- **Registro e array como tipo da linguagem** (`struct`, `let a = [0; n]`):
  sem produtor ainda. O desenho segue §2–§3 sem mudar o TIR: um registro é
  um `slot` do tamanho total e cada campo é `ptradd` constante — o
  validador já confere esses deslocamentos estaticamente.
- **Layout de *structs* do usuário com campos de trit** (como a PTE): a
  linguagem não tem acesso a campo de trit; hoje é aritmética explícita.
