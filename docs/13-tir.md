# TENERURA — Fase 8: TIR, a representação intermediária estruturada

Data: 2026-11-17 · sessão autônoma, **em cópia** (`~/tenerura-sandbox/`);
`~/tenerura/` não foi tocado. Vizinhos: `07` (ISA), `11` (T27R),
`12` (handoff). Código: `src/t27/src/lang/{ir,lower,irgen}.rs`,
`src/t27/src/bin/tir_bench.rs`, testes em `src/t27/tests/{tir,lang}.rs`.

Como no handoff, toda decisão aqui é **hipótese** com o experimento que a
derrubaria (§L). Números: uma máquina (56 threads), um dia.

## A. O pedido e o que foi entregue

Pedido: uma IR "inspirada na LLVM, mas melhor", projetada de fora para
dentro: ponteiros opacos com proveniência, regiões hierárquicas, argumentos
de bloco em vez de PHI, formato binário determinístico e de leitura
mínima, validação quase instantânea, tradução em uma passada, paralelismo
nativo, benchmarks.

Leitura adotada: no TENERURA, a IR tem um lugar natural — **entre a
checagem do T27R e o gerador de assembly**. Ela não substitui a LLVM em
geral; substitui o gerador de pilha que ia direto da AST para `.tas`
(`docs/11`, L3). Assim, cada requisito tem consumidor e teste reais.

| requisito | estado |
|---|---|
| controle estruturado hierárquico, sem PHI | **implementado** (§E) |
| argumentos de bloco | **implementado**: `yield` → resultados; laço recebe argumentos |
| tipos simples, ponteiro opaco, proveniência explícita | **implementado** (`word`, `ptr`, `slot`/`ptradd`/`load`/`store`); o T27R ainda não produz ponteiros — testado com módulo montado à mão |
| tipos estruturados de primeira classe | **não implementado**: especificado como próximo passo (§M) |
| UB claramente especificado | **implementado**: um único caso (§C.4) |
| semântica explícita em toda instrução | **implementado**: tipos de resultado e listas no próprio registro |
| formato binário determinístico, cabeçalho fixo, tabela de símbolos indexada | **implementado** (§G) |
| validação estrutural rápida e incremental | **implementado**: linear, por função (§H) |
| tradução direta em uma passada | **implementado** (§E) |
| paralelismo nativo | **implementado na compilação** (validação e geração por função); paralelismo *na semântica* (laços paralelos) **não** (§M) |
| benchmarks | **medido** (§K) |

## B. Arquitetura, de fora para dentro

```
Módulo ── cabeçalho fixo (32 B) ── entrada (índice da main)
  └─ tabela de funções (registros de 32 B, indexada; nome → tabela de strings)
       └─ função = faixa contígua de regiões + faixa contígua de instruções
            └─ árvore de regiões (raiz = 1ª região da faixa)
                 └─ região = argumentos + sequência de instruções + terminador
                      └─ instrução (16 B) = op · k · n · res · a · b
                           └─ valores: inteiros por função, definidos uma vez
```

Cada nível só conhece o de baixo por **índice**, nunca por ponteiro:
funções, regiões e instruções moram em tabelas planas (estilo Zig AIR /
"data-oriented"). Listas de tamanho variável (argumentos, tipos de
resultado, casos de `switch`) vão para uma tabela `extra` de `u32`. Em
memória e em disco a forma é a mesma; não há grafo de objetos para montar.

Por que nessa ordem: o que muda pouco e é caro de mudar (formato do
módulo, tabela de símbolos) foi fixado primeiro; o conjunto de instruções,
que vai crescer, é a camada mais interna e só precisa de opcodes novos.

## C. Semântica

### C.1 Valores e tipos

- `word`: inteiro balanceado de 27 trits, |v| ≤ M27 = (3²⁷−1)/2.
- `ptr`: par abstrato (alocação, deslocamento). **Opaco**: não há
  `ptrtoint` nem `inttoptr`; um `ptr` não pode virar `word` nem o contrário.

### C.2 Escopo (SSA lexical)

Um valor é visível do ponto de definição até o fim da região que o define,
incluindo regiões aninhadas depois dele. Dominância **é** escopo: não
existe árvore de dominadores, e a validação é uma passada. Cada valor é
definido exatamente uma vez na função.

### C.3 Avaliação

Instruções de uma região executam em ordem; a ordem é a única fonte de
sequenciamento de efeitos. Regras (σ = memória, ρ = ambiente de valores):

- `const c` → c. `bin k x y` → ALU auditada (`docs/05`): `wrap` em add/sub/
  mul, divisão centrada, **x/0 = 0 e x%0 = x** (total, como a ISA).
  Comparações dão 1 ou 0. `un neg/not/sgn`: −x; (x = 0 ? 1 : 0); −1/0/+1.
- `call f(args)`: avalia o corpo de `f` num ρ novo; argumentos já são
  valores (avaliação da esquerda para a direita feita por quem gerou o TIR).
- `intr k(args)`: efeito fixo por intrínseca (console, `yield`, relógio,
  IPC `call`/`recv`/`recv_val` — as mesmas chamadas do núcleo de `docs/08`).
- `if c {R₁} else {R₂}` → n resultados: executa R₁ se c ≠ 0, senão R₂; a
  região termina em `yield v₁..vₙ` (vira os resultados) ou sai da função.
- `switch x {casos} default R` → como `if`, escolhendo o braço cujo
  literal é igual a x, ou o default. Literais distintos e em ordem
  crescente (forma canônica; a ordem não muda o significado).
- `while (i₁..iₙ) cond(a…){C} body(b…){B}` → n resultados:
  ```
  v := (i₁..iₙ)
  repita: se C(v) termina em `cond c` com c = 0 → resultados := v; fim
          senão v := valores do `yield` de B(v)
  ```
  A condição não tem efeito sobre `v` (SSA); ela só *observa*.
- `return v` sai da função; `exit v` encerra a tarefa (não retorna).

### C.4 Comportamento indefinido — um único caso

`slot n` cria uma alocação nova de n palavras, viva até a função retornar,
com conteúdo inicial **indefinido mas estável** (ler antes de escrever dá
um valor qualquer, sempre o mesmo até a próxima escrita — não é UB).
`ptradd p k` preserva a alocação de p e soma k ao deslocamento (sem UB:
aritmética de deslocamento é total). **`load p` / `store p v` com
deslocamento fora de [0, n) da alocação de origem, ou depois de a função
dona retornar, é UB.** Não há outro: sem estouro indefinido, sem divisão
indefinida, sem `poison`/`undef`.

Consequência para otimização: como não existe conversão inteiro↔ponteiro,
a alocação de origem de todo `ptr` é determinada pelo fluxo de dados (ela
atravessa `if`/`while` só por resultados e argumentos de região, que o
otimizador vê). Dois ponteiros com `slot` de origem diferentes **nunca**
se sobrepõem — análise de alias exata e trivial, sem a ambiguidade de
proveniência que a LLVM carrega desde o `inttoptr`.

## D. Por que estes tipos (e não os da LLVM)

| LLVM | TIR | motivo |
|---|---|---|
| iN arbitrário, float, vetor | só `word` | a máquina tem uma palavra; tipos que o hardware não tem seriam mentira (`docs/11` L2) |
| `ptr` opaco + `inttoptr`/`ptrtoint` | `ptr` opaco **sem** conversão | proveniência nunca se perde; o modelo de memória LLVM ainda é discutido justamente por causa das conversões |
| `undef`, `poison`, `freeze` | não existem | a ALU é total; o único UB é de memória |
| GEP com tipos | `ptradd` em palavras | a memória é endereçada por palavra; não há layout a calcular |

## E. Controle estruturado e tradução em uma passada

A função é uma **árvore de regiões**; não há rótulo, salto ou bloco
básico. Isso dá duas coisas de graça:

1. **SSA sem algoritmo de SSA.** `lower.rs` mantém, para cada variável do
   T27R, "o valor corrente". Antes de `if`/`match`/`while`, uma varredura
   sintática (`assigned`) lista as variáveis de fora que o comando
   reatribui (respeitando sombreamento por `let`). Exatamente essas viram
   resultados do comando (e argumentos das regiões do laço). Não há PHI a
   inserir, nem PHI trivial a remover depois, nem fronteira de dominância.
2. **Validação sem CFG.** Dominância = escopo (§C.2).

`&&` e `||` viram `if` com um resultado (avaliação em curto-circuito
explícita). Código depois de `return`/`exit` no mesmo bloco não entra no
TIR. Exemplo real (`t27r --emit-ir`):

```
fn @main() -> (word) {
  %0 = const 0
  %1 = const 0
  %16, %17 = while (%0, %1) cond(%2, %3) {
    %4 = const 10
    %5 = lt %2, %4
    cond %5
  } body(%6, %7) {
    %8 = const 1
    %9 = add %6, %8
    %10 = const 2
    %11 = rem %9, %10
    %12 = const 0
    %13 = eq %11, %12
    %15 = if %13 {
      %14 = add %7, %9
      yield %14
    } else {
      yield %7
    }
    yield %9, %15
  }
  return %17
}
```

Fonte: `let mut i = 0; let mut s = 0; while i < 10 { i = i + 1; if i % 2 == 0 { s = s + i; } } return s;`

Diferença de semântica em relação ao gerador de pilha: o TIR fixa a
avaliação de argumentos **da esquerda para a direita** em toda chamada
(como Rust). O gerador antigo avaliava `call(cap, op, valor)` e `recv` de
trás para frente; só muda algo se dois argumentos tiverem efeito.

## F. Conjunto de instruções

Registro de 16 bytes: `w0 = op | k<<8 | n<<16`, `res`, `a`, `b` (u32 LE).
Campos não listados são **obrigatoriamente zero**.

| op | k | n | res | a | b |
|---|---|---|---|---|---|
| 1 Const | | | v | lo32 | hi32 (i64, \|·\| ≤ M27) |
| 2 Bin | add sub mul div rem eq ne lt le gt ge | | v | x | y |
| 3 Un | neg not sgn | | v | x | |
| 4 Call | | nargs | v (se a callee retorna) | índice da função | extra: args |
| 5 Intr | putc putdec yield time call recv recv_val | nargs | v (se tem resultado) | | extra: args |
| 6 If | | nres | v₀ | cond | extra: [then, else, ty×nres] |
| 7 Switch | | nres | v₀ | x | extra: [ncasos, nbraços, default, (lo,hi,braço)×ncasos, região×nbraços, ty×nres] |
| 8 While | | nres | v₀ | | extra: [cond, corpo, init×nres, ty×nres] |
| 9 Slot | | | p | palavras (1..3⁹) | |
| 10 PtrAdd | | | p | ptr | word |
| 11 Load | | | v | ptr | |
| 12 Store | | | | ptr | word |
| 13 Yield ⊣ | | nvals | | | extra: vals |
| 14 Cond ⊣ | | | | cond | |
| 15 Return ⊣ | | 0/1 | | val | |
| 16 Exit ⊣ | | | | código | |

⊣ = terminador: obrigatório como última instrução da região e proibido
antes dela. Terminador permitido por região: raiz → `return`/`exit`;
ramo de `if`/`switch` e corpo de `while` → `yield`/`return`/`exit`;
condição de `while` → só `cond`.

**Tipos de resultado ficam na instrução** (lista em `extra`), não são
inferidos dos `yield`: se todos os ramos saem com `return`, os resultados
ainda têm tipo, e a validação de quem usa não depende de olhar dentro.

## G. Formato binário

```
offset  conteúdo
0       "TIR\0"
4       versão = 1
8       nfunções · 12 nregiões · 16 ninstruções · 20 nextra · 24 nbytes de string · 28 entrada
32      funções   32 B cada: nome, tam_nome, assinatura(extra), rprimeira, rcontagem, iprimeira, icontagem, nvalores
…       regiões   16 B cada: args, nargs, primeira, tamanho
…       instruções 16 B cada
…       extra     4 B cada
…       strings   UTF-8, preenchidas com zero até múltiplo de 4
```

Tudo little-endian, sem varint, sem compressão: o deslocamento de qualquer
registro é aritmética sobre o cabeçalho. `decode` confere magic, versão,
tamanho total **exato**, opcodes e preenchimento, e copia as tabelas; não
interpreta nada. Determinismo: (1) o `lower` numera tudo em ordem de
criação, sem `HashMap` na saída; (2) campos não usados e preenchimento são
zero e a validação recusa o contrário; (3) faixas de funções ladrilham as
tabelas em ordem. Logo cada módulo válido tem **exatamente uma**
codificação, e `encode ∘ decode` e `decode ∘ encode` são identidade
(testado). O texto (`--emit-ir`) é só para leitura; não há parser de texto
— um formato canônico só.

> **Nota 2026-11-17 — versão 3: `extra` e `strings` canônicos.** A
> afirmação acima era falsa em v2: nada obrigava as listas a ladrilhar
> `extra` nem os nomes a ladrilhar `strings`. Lixo não referenciado e
> listas compartilhadas passavam, então o mesmo módulo tinha várias
> codificações, e uma lista compartilhada por *k* instruções multiplicava
> os valores definíveis (e a memória da validação) por *k*. Desde v3:
> nomes ladrilham `strings` em ordem (globais, depois funções); as listas
> ladrilham `extra` **função a função**: a assinatura e depois a lista de
> cada instrução, em ordem de índice, até a assinatura da próxima função
> (a primeira em 0; listas vazias seguem com deslocamento 0). O `lower`
> cria listas na ordem em que fecha as construções e termina com
> `Module::canonicalize`; `ir::list_len` é a fonte única do tamanho de
> cada lista. O `.tas` gerado não muda (conferido byte a byte nos 25
> pares fonte×modo da árvore). Ainda **não** são canônicas a ordem das
> regiões não raiz e a numeração dos valores: permutá-las dá outro módulo
> válido com a mesma semântica.

## H. Validação

Duas fases:

1. **Módulo** (`validate_module`): entrada válida; nomes UTF-8, não vazios,
   únicos; assinaturas dentro de `extra` e com tipos válidos; faixas de
   regiões e instruções das funções ladrilham as tabelas.
2. **Função** (`validate_func`), independente das outras: regiões ladrilham
   as instruções da função; a árvore parte da raiz e **cada região é usada
   exatamente uma vez** (árvore, não grafo; nenhuma órfã); terminadores só
   no fim e do tipo certo para a região; todo uso está no escopo e tem o
   tipo certo; todo valor `0..nvalores` é definido exatamente uma vez;
   constantes e casos em 27 trits; casos em ordem estritamente crescente;
   todo braço tem caso; campos não usados zero.

Custo: O(instruções + extra) por função, sem estrutura auxiliar além de
dois vetores de `nvalores`. **Incremental** no sentido literal: mudou uma
função, revalida-se a fase 2 dela (e a fase 1, que é O(funções)).

Entrada não confiável (o arquivo pode vir de qualquer lugar):

- **toda** mutação de 1 byte de um módulo real (4 por posição, 9 033 no
  teste) e todo truncamento terminam em `Ok`/`Err`, nunca em pânico; e
  todo módulo que a validação aceita passa pelo gerador sem pânico;
- esse teste achou **dois defeitos reais** durante a construção:
  `i64::MIN.abs()` estourando na checagem de constante, e `nvalores`
  corrompido pedindo alocação de até 4 G entradas (o teste levava 15 s;
  com o limite "nvalores ≤ o que a função consegue definir", 0,14 s);
- aninhamento limitado a `MAX_DEPTH = 1000` (a validação é recursiva).
- *nota 2026-11-17:* o limite "nvalores ≤ o que a função consegue definir"
  somava `Region.nargs`, que também vem do arquivo: com nvalores e um nargs
  corrompidos a validação pedia ~38 GB e o processo abortava (SIGABRT,
  fora do alcance de `catch_unwind`) — F14 da verificação, 4,8% de 100 000
  mutações de 1..4 bytes. Agora o limite é parâmetros + resultados das
  instruções + 2·n argumentos de região por `while`, contados depois de
  conferir que as listas ladrilham `extra`; cada valor definível
  corresponde a bytes do arquivo. Teste com o mesmo gerador e semente:
  `tests/tir.rs::mutacoes_de_varios_bytes_nunca_derrubam_nem_pedem_memoria`.

## I. Gerador TIR → `.tas`

`irgen.rs`: cada valor tem uma palavra no quadro; **constantes não ocupam
nada** e viram imediato no uso; os valores levados por um `while`
(argumentos da condição, do corpo e resultados) **dividem as mesmas
palavras**, então só o `yield` do corpo copia — como cópia paralela, com
pilha apenas se houver conflito (ex.: troca `a, b = b, a`). Argumentos de
chamada vão do quadro direto para `a1..a9`, sem `push`/`pop`.

## J. Paralelismo

Valores são numerados **por função**, e nada numa função aponta para
dentro de outra (chamadas usam o índice da tabela de símbolos). Logo
validação e geração são *embaraçosamente paralelas*: `par_map` divide as
funções em blocos contíguos entre `std::thread::scope` e concatena em
ordem de índice. O resultado não depende do escalonamento: testado que 1 e
8 threads dão o mesmo `.tas` e **o mesmo primeiro erro** num módulo com
dois defeitos. O `lower` continua sequencial (escreve em tabelas
compartilhadas; ver §M).

## K. Medições

### K.1 Código gerado (instruções de usuário executadas, núcleo real, I1–I12 conferidos)

Todo programa executável de `tests/lang.rs` agora roda **nos dois
geradores**; o teste exige saída e código de saída iguais e TIR ≤ pilha.

| programa | pilha | TIR | Δ |
|---|---:|---:|---:|
| `fib` 0..10 + impressão | 18 599 | 15 389 | −17% |
| laço soma 1..10 + impressão | 382 | 317 | −17% |
| fatorial + `match` de 3 vias | 317 | 274 | −14% |
| precedência e lógica | 211 | 207 | −2% |
| 5 parâmetros | 85 | 60 | −29% |
| `recv` | 33 | 26 | −21% |
| literal > 16 trits | 27 | 23 | −15% |
| `pingr` (com `pong.tas`) | 953 | 741 | −22% |

`pingr` em passos totais: 33 780 → 33 022 (−2%): o núcleo domina, como o
handoff já dizia (§1 de `docs/12`). O `.tas` estático do TIR é **maior**
(368 019 × 320 019 linhas no sintético): menos instruções executadas, mais
instruções escritas (cópias de `yield` explícitas).

### K.2 Compilação (`tir_bench 2000 7`, release, melhor de 7)

Entrada sintética: 2 000 funções com `while`, `match`, `if`, `&&`/`||` e
chamadas; 1,07 MB de fonte → 112 001 instruções TIR, 28 001 regiões.

| fase | ms | M instr TIR/s |
|---|---:|---:|
| fonte → AST checada | 24,3 | 4,6 |
| AST → `.tas` (gerador de pilha, antigo) | 19,5 | 5,8 |
| AST → TIR (`lower`) | 5,1 | 21,8 |
| validação, 1 thread | 3,8 | 29,6 |
| validação, 56 threads | 2,9 | 38,9 |
| encode | 0,8 | 139,5 |
| decode | 2,3 | 47,8 (1,15 GB/s) |
| TIR → `.tas`, 1 thread | 22,1 | 5,1 |
| TIR → `.tas`, 56 threads | 4,2 | 26,9 |

Leitura honesta:

- **Sequencial, o caminho TIR é mais lento** que o gerador antigo:
  lower + validação + geração = 31,0 ms × 19,5 ms. Em paralelo,
  12,2 ms. A IR se paga em código gerado (§K.1) e em ter um lugar para
  otimizar, não em velocidade do compilador sequencial.
- A validação é ~16% do tempo do `lower` + geração; "quase instantânea"
  vale como fração, não como absoluto.
- O binário tem 24,1 B/instrução e é **do mesmo tamanho que o texto**.
  A vantagem do formato é acesso O(1) e leitura sem parser, não tamanho.
- Com 20 000 funções a validação paralela escala (39,8 → 6,2 ms); com
  2 000, criar 56 threads come quase todo o ganho.
- Duas correções vieram da própria medição: `decode` alocava um `Vec` por
  registro (4,2 → 2,3 ms) e o gerador um `String` por linha (33 → 22 ms).

## L. Decisões como hipóteses

**T1. Árvore de regiões em vez de CFG.**
Por que: SSA e validação em uma passada; o T27R só tem controle
estruturado. Falsificação: o primeiro `break`/`continue`/`goto` ou um
front end com controle irredutível. `break` cabe como terminador novo
(sair de N níveis); controle irredutível não cabe e exigiria uma região
"CFG" como escape. Confiança: **alta** para o T27R, **média** como IR geral.

**T2. Ponteiro sem conversão para inteiro.**
Por que: alias exato e proveniência sem ambiguidade. Falsificação: um
alocador de memória ou o próprio núcleo escrito em T27R precisam fabricar
ponteiros a partir de endereços. Aí entra *uma* intrínseca explícita
(`ptr_from_addr`, com proveniência "externa" que pode ser alias de tudo),
nunca uma conversão silenciosa. Confiança: **média**.

**T3. Valores numerados por função, tabelas planas.**
Por que: funções independentes ⇒ validação/geração paralelas e
revalidação local. Falsificação: otimização interprocedural pesada
(inlining) precisa renumerar ao copiar corpos; é O(corpo), aceitável.
Confiança: **alta**.

**T4. Uma palavra de quadro por valor no gerador.**
Por que: simples e correto; já ganha da pilha. Falsificação: o programa
`fib` ainda faz `st`/`ld` do mesmo valor em sequência (comparação →
desvio). Um alocador de registros sobre o TIR deve cortar boa parte do que
sobrou; se não cortar >20%, a hipótese de que "o TIR é o lugar de
otimizar" fica fraca. Confiança: **média**.

**T5. Formato binário sem compressão.**
Por que: leitura sem parser. Falsificação: módulos grandes trafegando por
IPC de registradores (`docs/08`) — tamanho importaria mais que acesso.
Confiança: **média**.

**T6. Esquerda-para-direita em toda chamada.**
Por que: é a regra de Rust; o gerador antigo divergia em `call`/`recv`.
Falsificação: nenhuma prática — só importaria se alguém dependesse da
ordem antiga. Confiança: **alta**.

## M. O que ficou de fora (de propósito)

- **Tipos estruturados** (`struct` de `word`s como valor de primeira
  classe): sem produtor no T27R; o desenho natural é um tipo-tupla com
  `extract`/`insert` e lowering para várias palavras de quadro. Entra junto
  com `struct` na linguagem (handoff §7.6).
- **Paralelismo na semântica** (`parfor` como região): a máquina tem um
  núcleo e o T27R não tem tarefas; sem consumidor, seria código morto.
- **`lower` paralelo**: precisaria de `extra`/tabelas por função e
  rebase no fim; o `lower` é 5 ms em 2 000 funções.
- **Otimizações** (DCE, propagação de constantes, fusão compara-e-desvia,
  alocação de registros): o TIR existe para elas, mas nenhuma foi feita.
- **Interpretador de referência do TIR**: a semântica é conferida
  indiretamente (TIR → `.tas` → núcleo real × gerador antigo). Um
  interpretador permitiria fuzzing de programas sem o emulador.
- **Fuzzing de programas T27R** (handoff §7.8): continua aberto; o teste de
  mutação cobre bytes de módulo, não programas aleatórios.

## N. Como reproduzir

```sh
cd ~/tenerura-sandbox
tools/verificar-tudo.sh                    # suíte inteira, inclui lang (2 geradores) e tir
tools/t27r.sh src/os/user/pingr.t27r --emit-ir -o -     # TIR em texto
tools/t27r.sh src/os/user/pingr.t27r --tir -o /tmp/p.tas
nix-shell shell-rust.nix --run 'cd src/t27 && cargo run --release --bin tir_bench 2000 7'
nix-shell shell-rust.nix --run 'cd src/t27 && cargo test --test lang -- --nocapture'   # tabela K.1
```

## O. Adendo 2026-11-17 (Fase 9): formato v2 e o que entrou no TIR

Nota aditiva; o texto acima descreve a v1 e continua valendo onde não é
contrariado aqui. Motivação e medições em `docs/14`.

- **Cabeçalho de 40 bytes** (`VERSION = 2`): magic, versão, **flags**,
  nfunções, nregiões, ninstruções, nextra, nstrings, **nglobals**,
  entrada. `FLAG_PRIV` (bit 0) = módulo do núcleo; entrada `NO_ENTRY`
  (`u32::MAX`) = biblioteca. Outras flags são recusadas.
- **Tabela de dados externos** (12 B cada, logo após as funções): nome,
  tamanho do nome, tamanho em palavras (1..3⁹). Nomes únicos num espaço só
  com as funções.
- **`global g` → ptr** (op 17): ponteiro com alocação de origem = o dado
  externo g, de `size` palavras. Continua sem conversão inteiro→ponteiro.
- **`trap`** (op 18, terminador, todos os campos zero): para a execução de
  forma definida (`brk`, causa −6). Permitido em qualquer região.
- **`sh`** (binária k = 11): o deslocamento da ISA (docs/07 §4.2).
- **Intrínsecas 7..13** e **modo**: docs/16. A validação recusa
  intrínseca de usuário em módulo privilegiado e vice-versa, e `exit` no
  núcleo; `Intr.a` carrega o CSR (só `csr_*`; zero nas outras).
- **Limites por proveniência na validação:** para todo `load`/`store`, se a
  alocação de origem (`slot`/`global`) e o deslocamento acumulado por
  `ptradd` com constantes são conhecidos, o deslocamento tem de estar em
  [0, tamanho) — senão o módulo é inválido, **mesmo em código morto**. Por
  isso o `lower` transforma índice constante fora do array em erro de
  compilação (docs/14 §8, S5).
- **Backend:** biblioteca sem `_start`/`ulib`; rótulos `.L_<f>_<n>`,
  `.Lret_<f>`, `.Ltrap_<f>_<n>`; comparação usada só pelo `if`/`cond`
  seguinte vira `cmp` + desvio (sem 0/1 no quadro); constantes,
  comparações fundidas e argumentos de laço não ocupam palavra de quadro.

## P. Adendo 2026-11-17 (Fase 10)

Nota aditiva.
- `Bin` ganhou `k` = 12 `kmin`, 13 `kmax` e 14 `cmp`:
  - `kmin`/`kmax` são o mínimo/máximo trit a trit da ISA;
  - `cmp` é o sinal exato de x − y (−1/0/+1).

  O formato continua sendo a v2: só a faixa de `k` aceita cresceu.
- O backend reaproveita palavras de quadro entre regiões irmãs, como
  permite §C.2 (valor visível só até o fim da região). O quadro é o caminho
  mais fundo da árvore, não a soma.
- Motivo e medições: `docs/18` §18.
