# Tradutor LLVM IR → T27 — o que dá para entender até aqui

Data: 2026-11-17 · diretório: `~/tenerura-sandbox/llvm-bootstrap/` (nada escrito em `~/tenerura/`; de lá só se lê `src/os/lib/abi.inc`).

## 1. O que existe e roda de ponta a ponta

```
C  --clang -O0-->  .ll  --tradutor.py-->  .tas  --t27 asm-->  .sys  --t27 sys-->  código de saída
```

Cinco programas C passam por toda a cadeia e terminam com o código de saída
esperado (montador + núcleo real + emulador da sandbox):

| teste | o que exercita | esperado | obtido |
|---|---|---|---|
| `teste` | `while` + `if`, `add`/`sub`, chamada | 42 | **42** |
| `teste2` | `mul`, `icmp sle/sgt/slt/eq`, `&&` encadeado, bloco com 2 predecessores | 240 | **240** |
| `teste3` | `alloca [3 x i32]`, GEP, load/store indexado | 42 | **42** |
| `teste4` | string global, array global, ponteiro como parâmetro, índice variável | 51018101 | **51018101** |
| `teste5` | GEP constante como alvo de `store` e como argumento de chamada | 394 | **394** |

Comando de cada um: `t27 sys <teste>.sys 400000`.

## 2. O achado principal: o `icmp` estava invertido

A primeira versão do `icmp` gerava:

```
cmp  t3, a, b        ; t3 = sinal(a − b) = −1/0/+1
beq  t3, zr, falso   ; "se a == b, vá gravar 0"   <-- errado
```

O desvio levava ao rótulo do valor **falso** justamente quando a condição era
**verdadeira**: todo `icmp` saía negado.

**Por que o `teste` passou mesmo assim:** a inversão era dupla. Com `sgt`
invertido, o laço `while (b > 0)` nunca rodava, `multiplica(4,3)` devolvia 0,
e o `if (z == 12)` também invertido virava verdadeiro → `return 42`. O teste
dava o resultado certo pelo motivo errado. O `teste2` (que retorna um valor
calculado, não uma constante) expôs o defeito: devolvia 2 em vez de 240.

Correção (2 linhas): o desvio agora leva ao rótulo do valor **verdadeiro**
(`beq t3, zr, verdade`), e o caminho que cai grava 0.

Lição registrada: um teste cujo ramo falso devolve a mesma constante do ramo
verdadeiro **não testa a condição**. Precisa de um teste que devolva um valor
que só existe se o fluxo estiver certo.

## 3. Modelo do tradutor (o mais burro que funciona)

- **Toda** definição de valor LLVM (`%3`, parâmetro, temporário) vira **uma
  palavra na pilha**. Sem SSA, sem alocação de registradores, sem `phi`,
  sem `mem2reg`: o fluxo de dados passa sempre pela memória. O `-O0` já
  reescreve o que seria `phi` como `alloca` + `store` em cada aresta do CFG.
- Moldura no prólogo: `addi sp, sp, -N`; slots `0(sp)..(N-1)(sp)`; `lr` em
  `N-1(sp)`; argumentos derramados de `a1..a9`.
- `icmp` vira `cmp` + desvio condicional de dois operandos, materializando
  0/1 num slot; `br i1` lê esse slot com `bnz`.
- Globais (strings, arrays) vão para `.data` como `.word` por palavra;
  o endereço sai de `la`.

## 4. Fatos da ISA T27 que custaram tempo

1. `j` e `bnz` têm **2** operandos (`j L`, `bnz t1, L`); `beq/bne/blt/bge/bgt/ble`
   reais têm 3 (`reg, reg, label`). Não existe pseudo `beq a, b, L` genérica.
2. `cmp t, a, b` deixa **só o sinal** de `a − b` (−1/0/+1), não o valor.
3. Rótulos são **globais no arquivo**: blocos de funções diferentes colidem
   (`símbolo redefinido: L11`). Saída: prefixar com o nome da função.
4. `sp` de entrada da tarefa é `USER_SP = -364`; escrever em `0(sp)` **sem
   moldura** cai em −364, que é a guarda de nulo → trap de escrita. Por isso
   o prólogo não é opcional.
5. `mul` entrega a palavra baixa do produto (confirmado: 240×7 = 1680, 0×7 = 0).

## 5. Limites (o que isto NÃO é)

- Só `clang -O0`, só escalares `i8/i32/i64`, `icmp` com 6 condições.
- Sem `phi` explícito, sem `struct`, sem `float`, sem `switch`, sem varargs,
  sem otimização, sem análise de tipos (tudo é uma palavra de 27 trits).
- Não há verificação: `i64` não cabe em 27 trits — o tradutor aceita e o
  resultado é o wrap da máquina.
- Uma palavra por valor ⇒ molduras grandes; com página de pilha de 729
  palavras, funções muito grandes estouram a página (não testado).

## 6. Como reproduzir

```bash
cd ~/tenerura-sandbox/llvm-bootstrap
nix run nixpkgs#clang -- -S -emit-llvm -O0 teste.c -o teste.ll
python3 tradutor.py teste.ll -o teste.tas
~/tenerura-sandbox/src/t27/target/release/t27 asm teste.tas
~/tenerura-sandbox/src/t27/target/release/t27 sys teste.sys 400000
```

`teste.sys` é o manifesto: núcleo real + uma tarefa apontando para o `.tas`.

## 7. Pendências

- `README.md` ainda descreve só a Fase 1 (diz "não conhece GEP"); o código já
  faz GEP, globais e cast.
- Nenhum teste com moldura maior que uma página de pilha.
- Sem `switch`, `struct`, `float`, `phi` de verdade, `-O1+`.
