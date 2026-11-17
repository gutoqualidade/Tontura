# Programando para o núcleo TENERURA

Guia curto para quem quer escrever um programa que rode **dentro** do micronúcleo.
Detalhes completos: [`ISA.md`](ISA.md) (instruções da máquina) e [`T27R.md`](T27R.md)
(linguagem de alto nível). Tudo aqui foi executado; os exemplos estão em `src/os/`.

## 1. A máquina em 30 segundos

- Palavra de **27 trits balanceados** (cada trit é −1, 0 ou +1). Memória endereçada por
  palavra; um caractere de texto ocupa uma palavra.
- 27 registradores: `zr` (sempre 0), `a1..a9` (argumentos/resultados), `t1..t4`
  (temporários), `s1..s11` (preservados), `lr` (retorno), `sp` (pilha).
- Sem flags: `cmp` devolve −1/0/+1 num registrador; desvios comparam dois registradores
  (`beq bne blt bge bgt ble`, e os pseudo `bz bnz bn bp bnn bnp` que comparam com zero).
- Programas de usuário rodam em modo usuário, com memória traduzida; começam em
  `USER_TEXT` (365), com pilha em −364. A página 0 é guarda de ponteiro nulo.

## 2. Rodar um programa

Programa = um arquivo `.tas` (assembly) ou `.t27r` (alto nível, compilado para `.tas`).
Um **manifesto** `.sys` diz qual núcleo e quais tarefas formam o sistema, e que
capacidades cada tarefa recebe:

```
kernel ../kernel/kernel.tas
task ola ../user/ola.tas con:8=0        # slot 8 = console de depuração
```

```bash
nix-shell shell-rust.nix --run "cd src/t27 && cargo build --release"
src/t27/target/release/t27 sys src/os/sys/ola.sys        # -> ola, T27!
src/t27/target/release/t27 asm src/os/user/ola.tas       # só a listagem
```

Opções de capacidade no manifesto (`slot` = onde ela aparece para a tarefa):
`send:S=E/B` e `recv:S=E` (lados de um endpoint E; B = selo do remetente),
`con:S=0` (console), `task:S=N` (controlar a tarefa N), `irq:S=L` (reconhecer a linha L).
Outras: `fault=E/B` (avisos de morte vão para o endpoint E), `irqline=L:E/B`,
`dev=console@P`, `stack=N`. Veja `demo1.sys` e `demo2.sys` para sistemas maiores.

## 3. Assembly (`.tas`)

Formato `rótulo: mnemônico operandos ; comentário`. Seções `.text`/`.data`; diretivas
`.org .equ .word .zero .fill .string .stringz .page .entry .assert .include .macro`.
Pseudo-instruções: `li la mov neg sgn nop j call ret jr push pop`. Modelo mínimo
(`src/os/user/ola.tas`):

```asm
    .include "../lib/abi.inc"       ; constantes das chamadas de sistema
    .org USER_TEXT
_start:
    la a2, msg
    call puts                       ; de ulib.inc: a2 = texto terminado em 0
    li a2, 0
    j exit                          ; a2 = código de saída
    .include "../lib/ulib.inc"      ; putc, puts, putdec, exit
    .data
msg: .stringz "ola, T27!\n"
```

Instruções de máquina (resumo; tabela completa em `ISA.md` §4):
`add sub mul mulh div rem cmp sh kmin kmax addi shi lui ld st beq.. jal jalr sys brk`.
`div`/`rem` são **centrados** (resto em [−c/2, c/2]); `sh`/`shi` deslocam em trits
(contador negativo descarta trits baixos); `kmin`/`kmax` são o AND/OR de Kleene.

## 4. Chamadas de sistema (ABI)

Número em `a1`, argumentos em `a2..a9`, instrução `sys`. Retorno em `a1`:
`+1` feito, `0` nada aconteceu (ninguém do outro lado/prazo vencido), `−1` erro
(código em `a2`). O núcleo nunca segue ponteiro do usuário: tudo vai em registradores.
Não há autoridade ambiente: cada chamada nomeia uma **capacidade** (slot em `a2`).

| nº | chamada | uso |
|---|---|---|
| 0 | `SYS_YIELD` | cede o processador |
| 1 | `SYS_SEND` | a2=cap, a3..a9=mensagem (7 palavras), t1=paciência |
| 2 | `SYS_RECV` | a2=cap, t1=paciência → a2=selo, a3..a9=mensagem, t1=recibo |
| 3 | `SYS_CALL` | send + espera resposta (a3..a9) |
| 4 | `SYS_REPLY` | a2=recibo, a3..a9=resposta |
| 5 | `SYS_EXIT` | a2=código; não retorna |
| 6 | `SYS_SLEEP` | a2=passos |
| 7 | `SYS_TIME` | → a2=tempo |
| 8 | `SYS_PUTC` | a2=cap de console, a3=caractere |
| 9 | `SYS_TASK` | a2=cap de tarefa, a3: − mata, + reinicia, 0 consulta |
| 10 | `SYS_IRQ_ACK` | a2=cap de interrupção |
| 11 | `SYS_CAP_GRANT` | transfere uma capacidade a outra tarefa (move, nunca copia) |

Paciência (`t1`): negativa espera para sempre, 0 não espera, positiva espera esse
número de passos. Definições e códigos de erro: `src/os/lib/abi.inc`. Exemplo de
cliente/servidor: `ping.tas` e `pong.tas` (rodam em `demo1.sys`).

## 5. T27R (alto nível, subconjunto de Rust)

Um único tipo, `word`. Funções, `let`/`let mut`, `if/else`, `while`, `match` (com `_`
obrigatório), `return`. Embutidas: `putc putdec exit yield time recv recv_val sign call`.
Exemplo (`src/os/user/ola_t27r.t27r`):

```rust
fn main() -> word {
    putc(111); putc(108); putc(97); putc(32);   // "ola "
    let mut i = 1; let mut s = 0;
    while i <= 5 { s = s + i; i = i + 1; }
    putdec(s); putc(10);
    return 0;
}
```

```bash
tools/t27r.sh src/os/user/ola_t27r.t27r        # gera ola_t27r.tas ao lado
src/t27/target/release/t27 sys src/os/sys/ola_t27r.sys   # -> ola 15
```

O `.tas` gerado inclui `../lib/abi.inc`, então deve ficar em `src/os/user/`.

## 6. Onde mexer

| Quero... | Arquivo |
|---|---|
| ver/alterar o núcleo | `src/os/kernel/kernel.tas` |
| criar um programa | `src/os/user/*.tas` ou `*.t27r` + um `.sys` em `src/os/sys/` |
| entender as instruções | `docs/ISA.md`, `src/t27/src/isa.rs` |
| ver o montador / emulador | `src/t27/src/asm.rs`, `machine.rs` |
