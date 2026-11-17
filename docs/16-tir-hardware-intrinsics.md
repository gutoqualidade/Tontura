# TENERURA — Fase 9: intrínsecas de hardware `@tir.intr`

Data: 2026-11-17. Fonte de verdade da máquina: `docs/07` (ISA) e o
emulador `src/t27/src/machine.rs`. Nenhuma intrínseca aqui pressupõe
instrução, registrador ou mecanismo que não esteja lá.

## 1. Regras comuns

- Cada intrínseca é uma instrução `Intr` do TIR com número `k`, aridade e
  resultado **fixos** (tabela `INTRS` em `ir.rs`) e um **modo**: usuário
  ou núcleo. O módulo TIR tem a flag `FLAG_PRIV` (docs/13 §O); a validação
  **recusa** intrínseca do modo errado nos dois sentidos — instrução
  privilegiada em código de usuário trapearia (causa −2), e uma chamada de
  sistema dentro do núcleo seria um trap aninhado (pânico).
- **Efeito explícito.** Toda intrínseca é um efeito na ordem do programa:
  um otimizador não pode removê-la, duplicá-la nem trocá-la de lugar com
  outro efeito (outra intrínseca, `load`/`store`, `call`, `trap`). O TIR de
  hoje não reordena nada; a regra vale para o otimizador futuro.
- **Sem conversão inteiro→ponteiro.** Endereço físico entra como `word` e
  nunca vira `ptr` (§3).
- Texto: `@tir.intr.<nome>[<csr>](args)`.

## 2. Intrínsecas de usuário (existiam; agora com modo explícito)

| k | nome | semântica | `.tas` |
|---|---|---|---|
| 0 | `putc(c)` | escreve um caractere pelo console de depuração | `call putc` (ulib) |
| 1 | `putdec(x)` | escreve `x` em decimal | `call putdec` (ulib) |
| 2 | `yield() → st` | `SYS_YIELD` | `li a1, SYS_YIELD; sys` |
| 3 | `time() → t` | `SYS_TIME` | idem |
| 4 | `call(cap, op, v) → r` | `SYS_CALL`, paciência −1; r = `a3` se ok, senão o estado | docs/14 §6 |
| 5 | `recv(cap, pac) → st` | `SYS_RECV` | |
| 6 | `recv_val(cap, pac) → v` | `SYS_RECV`; `a3` se chegou mensagem | |

`exit` é o terminador `Exit` (só usuário).

## 3. Intrínsecas do núcleo (novas)

| k | nome | semântica (docs/07) | `.tas` |
|---|---|---|---|
| 7 | `csr_read<c>() → v` | v ← CSR c, c ∈ 1..12 | `csrr a1, c` |
| 8 | `csr_write<c>(v)` | CSR c ← v; c gravável (≠ 9 ipend, 10 irqn, 11 time) | `csrw c, t1` |
| 9 | `csr_swap<c>(v) → old` | **troca atômica** old ← c; c ← v | `csrx a1, c, t1` |
| 10 | `wfi()` | espera linha pendente **e** habilitada em IMASK, mesmo com IE = 0 | `wfi` |
| 11 | `tlbi()` | invalida traduções em cache | `tlbi` |
| 12 | `phys_read(a) → v` | lê a palavra no endereço **físico** a | `ld a1, 0(t1)` |
| 13 | `phys_write(a, v)` | escreve v no endereço físico a | `st a1, 0(t1)` |

O CSR é **parte da instrução** (campo `a` do `Intr`), não um operando: no
T27R tem de ser literal, e a validação confere existência e escrita.

### 3.1 Endereço físico, MMIO e proveniência

- `phys_*` só existem no núcleo, onde a ISA não traduz endereços (docs/07
  §6). O endereço é uma `word`; **nada** produz `ptr` a partir dele.
- **Validade:** o endereço tem de designar RAM que o TIR não gerencia
  (quadros, tabelas de página, imagens de tarefa) ou uma página de
  dispositivo. Acessar por `phys_*` uma alocação do TIR (`slot`) ou um
  `global` é **UB**: o otimizador pode supor que `phys_*` não sobrepõe
  nenhum `ptr`. Endereço inexistente: a máquina dá erro de barramento
  (causa −7) — comportamento da máquina, não do TIR.
- **MMIO = `phys_*` numa página de dispositivo.** Semântica de
  dispositivo (ler `RX` consome, escrever `TX`/energia tem efeito): por
  isso toda `phys_*` é volátil (§1) — nunca removida nem fundida, mesmo
  lendo o mesmo endereço duas vezes.
- **Ordem de memória:** um núcleo, execução em ordem, sem cache visível
  no emulador: os efeitos acontecem na ordem do programa. Não existe
  barreira na ISA, e **o TIR não oferece `fence`** — oferecer seria
  inventar uma instrução. Se a ISA ganhar memória concorrente, `fence`
  entra junto com o modelo (docs/07 §8, [A]).
- **Tempo de vida** de uma referência MMIO: enquanto a página existir na
  máquina (sempre, para console e energia). O TIR não tem referência
  MMIO como valor — só o endereço numérico.

### 3.2 Interrupções

O estado de interrupção é o CSR STATUS = MODE + 3·IE + 9·PMODE + 27·PIE
(docs/07 §5) e a máscara IMASK (um trit por linha). As cinco operações
pedidas são composições de §3, sem intrínseca nova:

| operação | em T27R |
|---|---|
| ler estado | `let s = csr_read(1);` |
| desabilitar e guardar | `let s = csr_read(1); csr_write(1, s - 3 * ((s / 3) % 3));` — ou atômico: `csr_swap(1, …)` |
| restaurar | `csr_write(1, s);` |
| habilitar | `csr_write(1, s + 3)` se IE = 0 |
| mascarar linhas | `csr_write(8, máscara)` (é o que `irq.t27r` faz) |

**Retorno de interrupção (`eret`) não é intrínseca**: ele sai da função
sem passar pelo epílogo, com outra pilha e outros registradores. Fica na
camada 0 (docs/14 §1, §6).

## 4. Recusadas, com motivo

| pedido | por que não |
|---|---|
| `halt` | não há instrução. Desligar = `phys_write(DEV_POWER, código)`; é o que o núcleo faz |
| atômicas de memória (CAS, fetch-add) | não há instrução; a ISA só tem `csrx` (troca atômica de CSR), exposta como `csr_swap` |
| barreiras de memória | não há instrução nem modelo concorrente (§3.1) |
| `eret` / troca de contexto | camada 0 (§3.2) |
| operações de tabela de páginas | não há instrução própria: PTE é `phys_write` da palavra (docs/15 §4) + `tlbi`. O que existe está coberto |
| proteção de memória além da paginação | não existe na máquina |
| intrínsecas de MMIO para usuário | um driver de usuário vê o dispositivo mapeado no seu espaço virtual; hoje esse driver é assembly (`ulib_drv.inc`). Uma intrínseca `dev_read/dev_write` de usuário fica para quando houver driver em T27R |

Qualquer outro `k` é recusado pela validação (`intrínseca k`).

## 5. Evidência (`tests/tir_system.rs`)

- máquina nua, camada 0 de 8 instruções → `fn_kmain` em TIR: escreve
  "TIR" no console por `phys_write`, faz `csr_write`/`csr_read`/
  `csr_swap` no SCRATCH, `tlbi`, lê de um `static` com índice de execução:
  desliga com 25 467 = 12 345 + 12 345 + 777;
- `wfi` com IE = 0 e TIMECMP = TIME + 50 volta depois de 50..59 passos;
- intrínseca no modo errado recusada pela linguagem **e** pelo validador
  (flag trocada à mão nos dois sentidos); CSR só-leitura recusado;
- no núcleo real: `csr_write(8, …)` em `irq.t27r` substitui o `csrw imask`
  das rotinas em assembly, com toda a suíte do núcleo verde.

Não testado: `phys_read` de dispositivo com efeito (RX do console) a
partir de TIR — o caminho de leitura é o mesmo `ld` testado com RAM.
