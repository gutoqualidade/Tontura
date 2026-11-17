# llvm-bootstrap — Fase 1 do tradutor LLVM IR → T27

Tradutor 1-para-1 do LLVM IR **ingênuo** (`clang -O0`) para assembly da máquina
ternária T27 do Tenerura. Não é compilador: não otimiza, não aloca registradores,
não conhece `phi`, `GEP`, `mem2reg`, structs nem vetores.

## Resultado

```
soma.c  ->  soma.ll  ->  tradutor.py  ->  soma.tas  ->  t27 sys soma.sys
                                                        saiu com código 12
```

`main` retorna `soma(5,7) = 12`, e o código de saída da tarefa é 12. Pipeline
inteiro provado: C → LLVM IR → assembly T27 → montador → micronúcleo → emulador.

## Arquivos

| Arquivo | O que é |
|---|---|
| `soma.c` / `soma.ll` | fonte e IR `-O0` |
| `tradutor.py` | o tradutor (Python, ~250 linhas) |
| `soma.tas` | saída gerada |
| `soma.sys` | manifesto: núcleo real + a tarefa `soma` |
| `saida.txt` | execução oficial, colada |
| `diferenca.{c,ll,tas,sys}` | teste extra do caminho `sub` (5−7 = −2) |
| `soma_errado.tas` | teste negativo: cabeçalho do prompt, sem `mov a2, a1` |

## Como rodar

```bash
cd ~/tenerura-experiments/llvm-bootstrap

nix run nixpkgs#clang -- -S -emit-llvm -O0 soma.c -o soma.ll
python3 tradutor.py soma.ll -o soma.tas

# binário já compilado do projeto (execução somente-leitura; nada é escrito
# em ~/tenerura — o cargo/build roda no target/ de lá, então não se usa)
T27=~/tenerura/src/t27/target/release/t27
$T27 asm soma.tas
$T27 sys soma.sys 400000
```

## Modelo de tradução

Uma palavra T27 por **valor LLVM** (parâmetro, `alloca` ou temporário SSA).
Nada de registrador: cada `%n` tem um slot fixo na moldura.

```
prólogo:  addi sp, sp, -N       ; N = params + allocas + temporários + lr
          st lr, N-1(sp)
          st a1, slot(%0)(sp)   ; argumentos da ABI T27 derramados
corpo:    ld t1, slot(%x)(sp)
          add t3, t1, t2
          st t3, slot(%y)(sp)
epílogo:  ld a1, slot(ret)(sp)
          ld lr, N-1(sp)
          addi sp, sp, N
          ret
```

Custo: ~2,5 instruções por instrução LLVM. É o esperado para um tradutor que
não faz nem mem2reg.

## Duas correções ao roteiro original

1. **`SYS_EXIT` recebe o código em `a2`, não em `a1`** (`abi.inc`: `SYS_EXIT, 5 ;
   a2 = código de saída`). O cabeçalho proposto no prompt
   (`call main; li a1, SYS_EXIT; sys`) sai com lixo em `a2` — comprovado em
   `soma_errado.tas`: código de saída **7**, não 12. O correto é
   `call main; mov a2, a1; li a1, SYS_EXIT; sys`.
2. **A moldura move `sp`; não dá para usar `-1(sp)` com `sp` parado.** `push`
   numa função chamada desce para exatamente `-1(sp)` do chamador e sobrescreve
   o local. Por isso o prólogo faz `addi sp, sp, -N` e os slots são
   `0(sp)..(N-1)(sp)` — equivalentes a `-N..-1` em relação ao `sp` de entrada.

## Limites conhecidos (Fase 2)

- sem `phi`, `br`, `icmp`, `select`, `getelementptr`, `mul`, `div`, `rem`
- sem `i8`/`i16`/`i64`, sem ponteiros de verdade (só `alloca i32`), sem arrays
- sem `printf`: o programa não tem console; só o código de saída prova o valor
- `-O0` obrigatório: com qualquer otimização o IR sai do subconjunto

## Fase 3 estendida — `teste4` (2026-11-16)

```
teste4.c -> teste4.ll -> tradutor.py -> teste4.tas -> t27 sys teste4.sys
                                                      saiu com código 51018101
```

String global, array global, ponteiro como parâmetro, índice variável:
`((comprimento("Hello")·100 + soma(tabela,4))·100 + soma(local,4))·1000 + msg[1]`
= `((5·100 + 10)·100 + 18)·1000 + 101`.

O que faltava: `msg[1]` chega como **GEP em expressão constante** dentro do
`load` (`load i8, ptr getelementptr inbounds ([6 x i8], ptr @msg, i64 0, i64 1)`).
`limpar()` agora troca essa expressão pelo operando `@msg+1`, e `carregar()`
emite `la t1, G_msg + 1`. Vale para `load`, `store` e argumentos de `call`.

Conferido: a saída do tradutor para `soma`, `diferenca`, `teste`, `teste2` e
`teste3` é idêntica antes e depois da mudança, e os cinco continuam saindo com
12, −2, 42, 240 e 42.

Limite novo, deliberado: só a forma tipada de dois índices é aceita. A forma em
bytes (`getelementptr (i8, ptr @g, i64 K)`) continua recusada, porque K viria em
bytes e o modelo de memória aqui é uma palavra por elemento.

A seção "Limites conhecidos" acima descreve a Fase 1; `mul`, `icmp`, `br`, GEP e
globais já são aceitos (ver o cabeçalho de `tradutor.py`).

`teste5` confere os outros dois lugares onde o GEP constante aparece — alvo de
`store` (`tabela[2] = 9`) e argumento de `call` (`comprimento(&msg[2])`): sai
com 394 (= 3·100 + 9·10 + 4). Os `smoke_*` continuam como estavam (1680, 0, −1;
`smoke_sp` falha de propósito ao retornar com `lr` = 0).
