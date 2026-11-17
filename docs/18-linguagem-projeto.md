# TENERURA — Fase 10: projeto da linguagem de sistemas T27R (alvo completo)

Data: 2026-11-17 · em cópia (`~/tenerura-sandbox/`). Vizinhos: `07` (ISA,
fonte de toda afirmação de máquina), `08` (núcleo), `11` (T27R hoje), `13`
(TIR), `14` (ABI), `15` (layout), `16` (intrínsecas), `17` (avaliação de 53
capacidades).

**Estado deste documento: PROJETADO.** Ele fixa a linguagem *final* e a
ordem de construção. O que já existe está marcado **[E]**; o resto é
**[P]** (projetado aqui) ou **[H]** (hipótese que depende de medição). §15
diz o que entra primeiro; §16 diz o que derrubaria cada escolha.

**Implementado nesta sessão: a primeira fatia da etapa 1** (§18). Ela
cobre:
- `bool`/`trit`/`word` distintos;
- `<=>`;
- `&` e `|` de Kleene;
- `match` de trit exaustivo;
- funções sem valor de retorno;
- palavras reservadas;
- quadro por escopo no backend.

O resto continua projeto.

## 0. Resumo

O `docs/17` respondeu "qual é o menor núcleo que dá para construir agora".
Este documento responde à pergunta seguinte: **qual linguagem o TENERURA
deve ter quando estiver pronta**. As etapas iniciais só implementam partes
dela. Nenhuma etapa pode tomar uma decisão que a feche (§15.1).

A linguagem se apoia em cinco ideias:

1. **Inteiros são uma família de larguras em trits, não de bits.** `tN`
   vai de `t1` (= `trit`) a `t27` (= `word`). Nenhum tem sinal à parte e a
   negação nunca estoura. Truncar para N trits é **um** `rem` por 3ᴺ, e
   separar campos é exato (§4.1).
2. **A decisão de três vias é nativa.** `a <=> b : trit` é **uma**
   instrução (`cmp`). `match` sobre `trit` é exaustivo com 3 braços e vira
   2 desvios. `&` e `|` sobre trits são `kmin`/`kmax` (Kleene).
3. **Palavras codificadas pelo sinal viram tipos.** O sistema está cheio
   delas:
   - causa de trap: falha / chamada / interrupção;
   - paciência: para sempre / não espera / passos;
   - operação de `task`: mata / consulta / reinicia;
   - selo: núcleo / cliente.

   O **enum de faixas** (§4.3) dá nome a cada uma. O layout continua sendo a
   própria palavra e o `match` continua sendo o desvio pelo sinal.
4. **A autoridade é tipada, e a linguagem confere o que o núcleo já
   confere.**
   - Capacidades são tipos lineares; protocolos de IPC são declarações.
   - O recibo de resposta é linear: tem de ser usado exatamente uma vez,
     o que o núcleo hoje só confere em execução.
   - O modo núcleo é uma propriedade do módulo.
   - O limite de pilha do núcleo vira erro de compilação.
5. **Segurança de memória sem coletor e sem anotar tempos de vida.**
   Valores são mutáveis e se copiam. As referências são *vistas* que não
   escapam. Referências guardáveis são índices de arena com geração,
   conferidos em execução, como a geração de tarefa que o núcleo já usa.
   O resto fica em `unsafe` explícito.

O compilador é escrito na própria linguagem. O critério de
auto-hospedagem (§12) é mais forte que o do `docs/17`: o TIR canônico
gerado pelo compilador em Rust (estágio 0) e pelo compilador em T27R
(estágio 1), a partir do mesmo fonte, tem de ser **idêntico byte a byte**.
Assim, duas implementações independentes conferem uma à outra.

## 1. O que "a melhor possível" quer dizer aqui

Critérios, em ordem de desempate:

1. **Verdade sobre a máquina.** Toda operação primitiva corresponde a um
   comportamento documentado no `docs/07`. Não existe "unsigned", byte,
   complemento de dois nem ponto flutuante simulado em silêncio.
2. **Nenhum custo escondido.** Nada aloca, desvia, desempilha ou converte
   sem que o código diga. O custo de cada construção é previsível em
   instruções.
3. **Seguro por padrão, controle total sob `unsafe` auditável.**
4. **O que o sistema confere em execução, a linguagem confere antes**
   (capacidades, recibos, modo, pilha). O núcleo continua sendo a
   autoridade: a linguagem é a primeira linha de defesa, não a única.
5. **Análises de custo linear sobre a árvore estruturada.** Sem inferência
   global, sem resolvedor de tempos de vida e sem busca de instâncias com
   retrocesso. Isso não é concessão ao bootstrap: compilação rápida, erro
   perto da causa e um compilador pequeno o bastante para ser verificado
   são propriedades desejáveis em si. Que isso também torne a
   auto-hospedagem viável é consequência.
6. **Uma autoridade semântica**: o TIR (`docs/13`), com o interpretador de
   referência. A linguagem se define pela tradução para o TIR.

## 2. A ISA e onde a linguagem a usa

| ISA (`docs/07`) | construção | código |
|---|---|---|
| palavra balanceada, sem "unsigned"; negação exata | família `tN`; `-x` total | `sub a, zr, b` |
| `cmp` → trit | `a <=> b`, `x.sign()`, `match` de trit | `cmp` + `bn`/`bp` |
| `kmin`/`kmax` | `&`, `\|` em `trit`; elemento a elemento em `packed [trit; n]`; `&&`, `\|\|` puros sem desvio | 1 instrução |
| `div`/`rem` centrados | `/`, `%`; `tN::wrap(x)` = `rem x, 3ᴺ` | 1 instrução |
| `sh` com sinal | `x << k` = ×3ᵏ; `x >> k` = ÷3ᵏ arredondado (= descartar k trits, provado) | `shi` |
| `addc` | `a.add_carry(b) -> (word, trit)`; `checked_add` | `add` + `addc` |
| `mul`/`mulh` | `a.mul_wide(b) -> (word, word)`; ponto fixo de escala 3ᵏ | 2 instruções |
| `lui`+`addi` sem empréstimo | qualquer constante de 27 trits | ≤ 2 instruções |
| `ld`/`st` base + imediato de 16 trits | campo de registro em 1 instrução; registro e array **centrados** (§4.3) | 1 instrução |
| desvios de 2 destinos | condição é `bool` | 1 desvio fundido |
| `jal`/`jalr` | ponteiro de função; `dyn`; tabela de saltos **centrada** (sem subtrair a base) | `jalr` |
| `sys` + `a1..a9` | protocolos; o ABI de `Outcome` é o protocolo de registradores do núcleo | 0 cópias no caso comum |
| sinal de CAUSE | `enum Cause: word { Fault(..=-1), Syscall = 0, Irq(1..) }` | 2 desvios |
| `csrr`/`csrw`/`csrx` | intrínsecas; `csr_swap` é a única atômica | 1 instrução |
| `wfi`, `tlbi`, `eret` | intrínsecas; `eret` só em `naked fn` (§7.3) | 1 instrução |
| página de 729 centrada; VA de 18 trits | `PageOff = t6`, `VAddr = t18`, `Pte` como registro `packed` | `rem`/`shi` |
| registradores com sinal = classe de preservação | ABI v2 (§11.2) | — |
| TIME modular | tipo `Time` sem `<`: só diferença reduzida (o defeito F2 vira erro de tipo) | `sub` |
| opcode 0 ilegal; memória zerada não executa | estáticos zerados; o nicho de "nenhum" é 0 (§5.6) | — |

## 3. Sintaxe

Continua com a família de Rust, porque o leitor já a conhece. A gramática
continua LL(1) e tem um **orçamento de palavras-chave**: cada recurso novo
declara quantas acrescenta.

Hoje são 12 palavras-chave **[E]**. O alvo tem 36:
- as 12 de hoje;
- `struct enum impl trait pub mod use const unsafe loop for in break
  continue defer become as type packed naked protocol never self Self`.

`view`, `linear` e `where` são contextuais.

Arquivo = módulo. Atributos ficam em `#[...]` e o modo do módulo em
`#![mode(kernel)]`. Comentários de documentação usam `///`.

## 4. Tipos

### 4.1 Inteiros: a família `tN`

- `tN` para N = 1..27, com faixa ±(3ᴺ−1)/2. `t27` = `word` **[E]** e
  `t1` = `trit`.
- Fora de registros `packed`, todo `tN` ocupa uma palavra.
- Aliases do sistema:
  - `Reg = t3` (−13..13);
  - `PageOff = t6` (±364);
  - `VAddr = t18`;
  - `Line = t4` (linhas de interrupção).
- **Aritmética:** `+ - *` sobre `tN` resultam em `word`, com o wrap
  balanceado da ALU **[E]** (`docs/05`). `/` e `%` são centrados, e `x/0 = 0`
  e `x%0 = x` (a ISA é total) **[E]**. A semântica do `word` **não muda**.
- **Conversões:**
  - Alargar (`tN → tM`, M ≥ N) é implícito, porque o valor não muda.
  - Estreitar é sempre explícito:
    - `x as t6` confere e para em `trap` se não couber;
    - `t6::try_from(x) -> Option<t6>`;
    - `t6::wrap(x)` = trits baixos = `rem x, 729`.

  Truncar para N trits **é** o resto centrado por 3ᴺ: 3ᴺ é ímpar, logo o
  resto com |r| ≤ (3ᴺ−1)/2 é único e são exatamente os N dígitos baixos.
  Em binário, truncar e `srem` divergem; aqui coincidem.
- **Operações largas:**
  - `add_carry` (`add` + `addc`): o carry é um trit;
  - `mul_wide` (`mul` + `mulh`);
  - `checked_*`;
  - `wrapping_*` (o operador comum já é este).
- Por que não `i8/i16/u32`: as larguras da máquina são em trits, e as
  faixas úteis do sistema são potências de 3 (página, VA, número de
  registrador). Faixa no tipo dá três coisas:
  - elimina verificação de limite (§4.3);
  - dá nicho para `Option` (§4.3);
  - documenta o campo.

### 4.2 `bool`, `trit` e lógica de três valores

- `bool` = {`false`, `true`}, representado por 0 e 1. **É o único tipo
  aceito por `if`/`while`**, porque a máquina só desvia para dois
  destinos. Comparações devolvem `bool`. `&&`, `||` e `!` operam sobre
  `bool`. Com operandos sem efeito, `&&`/`||` viram `kmin`/`kmax`, sem
  desvio: em {0,1}, mínimo e máximo trit a trit são E e OU.
- `trit` = {−1, 0, +1}. Lê-se como sinal **e** como valor de verdade de
  Kleene:
  - `&` = `kmin`;
  - `|` = `kmax`;
  - `-` = negação (exata);
  - `match` sobre `trit` é exaustivo com os braços −1, 0 e 1, sem `_`.
- **Nenhum trit vira condição implícita.** Decidir em três vias é sempre
  `match`, para que "0 = indeterminado" nunca seja lido como falso em
  silêncio. Mantém a regra do `docs/17`.
- `a <=> b : trit` é `cmp`. `x.sign()` é `sgn` **[E, como `sign(x)`]**.
- **Mudança explícita em relação ao T27R de hoje:** hoje "qualquer valor
  não nulo é verdadeiro" (`docs/11` §B). No alvo, `if x` com `x: word` é
  erro de tipo e se escreve `if x != 0`.
  - Por quê: tipagem forte (`docs/17`, "adopt") e o trit não podem
    conviver com verdade implícita.
  - Migração: os programas existentes já condicionam por comparação;
    falta o gerador de *fuzzing* (§15, etapa 1).

### 4.3 Tipos compostos

- **Tuplas** `(A, B)` e **registros** `struct`.
  - Layout: campos em ordem, sem preenchimento (`docs/15` §2) **[E regra]**.
  - `#[centered]` põe o ponteiro na palavra do meio. Deslocamentos ±k
    cabem no mesmo imediato (±21 523 360 palavras). O TCB do núcleo já é
    assim.
- **Arrays** `[T; n]`, com índice de 0 a n−1 e verificação de limite **[E
  para `static`]**.
- **Arrays de índice balanceado** `[T; R]`, onde R é um tipo de faixa
  (`t3`, `-13..=13`).
  - O elemento 0 fica no meio, então o endereço é base + i, sem subtrair
    nada.
  - Um índice do tipo R **dispensa** a verificação.
  - Exemplo: a área de registradores do TCB é `[word; Reg]`, indexada pelo
    número do registrador, exatamente como o `docs/15` §4 descreve em
    assembly.
- **Enums com dados**.
  - Discriminantes centrados na ordem de declaração: n = 3 dá {−1, 0, +1}.
    Também podem ser explícitos.
  - Layout: o discriminante (1 palavra) seguido da maior variante.
  - **Nicho:** se uma variante tem dados com valores impossíveis, o
    discriminante mora lá e o enum não cresce:
    - referência: 0;
    - `tN` com N < 27: qualquer valor fora da faixa;
    - `bool`: qualquer valor diferente de 0 e 1.
  - `match` é exaustivo.
  - Com até 3 variantes, o despacho são 2 desvios sobre o sinal. Com mais,
    é uma tabela de saltos centrada.
- **Enum de faixas** (`enum E: word { … }`): cada variante é uma faixa
  disjunta de valores de **uma** palavra, que é o próprio layout.

  ```
  enum Cause: word { Fault(c: -7..=-1), Syscall = 0, Irq(l: Line in 1..=27) }
  enum Patience: word { Forever = ..=-1, Poll = 0, Steps(n: 1..=3^20) }
  ```

  Construir confere a faixa. O `match` compila para comparações com as
  fronteiras; se uma fronteira é 0, para o desvio pelo sinal. As palavras
  sinalizadas da ABI do núcleo (`docs/08` §3) ganham nomes sem mudar um
  trit.
- **`Option<T>`, `Result<T, E>` e `Outcome<T, E>`** ficam na biblioteca
  central.
  - `Outcome` = `Fail(E) = −1 | Empty = 0 | Done(T) = +1` é a forma de
    **todo** resultado do núcleo (erro / nada aconteceu / feito).
  - Pelo ABI (§11.2), o discriminante vai em `a1` e os dados em `a2..`.
    É exatamente o que `sys` já devolve, então o invólucro da chamada de
    sistema não copia nada no caso comum.

### 4.4 `packed`: campos de trit numa palavra

```
packed struct Pte {           // docs/07 §6, do trit 0 para cima
    kind:  trit,              // +1 folha, −1 tabela, 0 ausente
    write: bool,
    exec:  bool,
    _:     t3,                // zero (o construtor exige)
    frame: t21,               // centro do quadro = frame·729
}
```

- A soma das larguras é ≤ 27 trits.
- **Ler** o campo de largura w na posição p é `rem(shi(x, −p), 3ʷ)`: 2
  instruções exatas.
- **Escrever** é somar (novo − velho)·3ᵖ.
- `packed [trit; n]` (n ≤ 27) admite `&`, `|` e `-` **elemento a
  elemento** numa instrução: são 27 operações de Kleene por `kmin`.
- A máscara de interrupções (um trit por linha) passa a ser
  `packed [trit; 27]`, e `imask[l] = 1` gera a mesma aritmética que
  `irq.t27r` escreve à mão hoje.

### 4.5 Genéricos e traits

- **Genéricos monomorfizados** em funções e tipos. Constantes genéricas
  (`[T; N]`) são de tipo `word`, `tN` ou `bool`.
- **Traits nominais** com métodos, tipos e constantes associados. O corpo
  genérico é conferido **na definição**.
  - Coerência: a regra do órfão.
  - Fica de fora: especialização, tipos de ordem superior e busca com
    retrocesso.
- `dyn Trait` só existe como vista (§5.3): par (dados, vtable estática),
  sem alocação.
- Operadores por trait (`Add`…), para os numéricos da biblioteca (ponto
  fixo 3ᵏ, inteiro grande sobre `add_carry`/`mul_wide`).
- `#[derive(...)]` de um conjunto fixo (`Copy`, `Eq`, `Ord` com `<=>`,
  `Debug`), feito pelo compilador.

### 4.6 Tipos do sistema (biblioteca central, com nomes que o compilador conhece)

| tipo | o que é | conferido |
|---|---|---|
| `Time` | relógio modular | sem `<`; `t.since(t0) -> word` reduzido à janela; `deadline.passed()` |
| `Client<P>`, `Server<P>` | capacidade de envio/recepção de endpoint, protocolo P | linear; o tipo vem do protocolo |
| `Reply<R>` | recibo de chamada | **linear**: responder, guardar (resposta adiada) ou `abandon` explícito |
| `TaskCap`, `IrqCap`, `ConsoleCap` | os outros tipos de capacidade (`docs/08` §2) | lineares |
| `Phys` | endereço físico (só núcleo) | não vira `&T`; acesso só por `unsafe` |
| `Mmio<R>` | bloco de registradores de dispositivo | todo acesso é volátil; campos `ro`/`wo` |
| `Idx<T>` | índice de arena com geração | §5.4 |

**`main` recebe as capacidades iniciais tipadas**, por exemplo
`fn main(srv: Server<Contador>, con: ConsoleCap)`. O construtor de imagem
confere cada slot do manifesto (`.sys`) contra a assinatura, então um
manifesto errado vira erro de construção, não falha em execução.

## 5. Memória e posse

### 5.1 Onde a memória mora

| classe | origem | dono | tempo de vida |
|---|---|---|---|
| quadro | `slot` do TIR **[E]** | a função | até o retorno |
| estática | `static` / `static mut` **[E]** | o programa | a imagem |
| arena | biblioteca, sobre uma estática ou sobre memória concedida pelo núcleo (futuro `map`) | quem tem a arena | até `reset` (geração +1) |
| externa | `unsafe`: físico, MMIO, página concedida | o código que a adota | declarado |

**Não existe alocação implícita** **[E, vira regra]**: nenhuma construção
aloca fora do quadro sem uma chamada visível.

### 5.2 Valores, cópia e tipos lineares

- **Semântica de valor**: atribuir e passar copiam. O custo é o tamanho do
  tipo em palavras, sem destrutor.
- **Tipos lineares** (`linear struct`, e toda capacidade e todo `Reply`)
  não se copiam. São movidos e **consumidos exatamente uma vez**.
  - Consumir é passar por valor a uma função. As funções de descarte são
    declaradas pelo próprio tipo, por exemplo `Reply::abandon`.
  - A verificação é uma passada sobre a árvore estruturada: na saída de
    cada região, o conjunto de lineares vivos tem de estar vazio, e os
    braços de `if`/`match` têm de concordar.
- Sem RAII. A limpeza explícita é `defer` (§6).

### 5.3 Vistas: `&T`, `&mut T`, `&[T]`, `&dyn Tr`

- **Onde uma vista pode morar:**
  - parâmetro;
  - local;
  - campo de um `view struct`, que também é segunda classe, como o
    `ref struct` do C#;
  - resultado de função.
- **Onde ela não pode morar** (é recusado):
  - estática;
  - campo de registro comum;
  - elemento de array ou enum comum;
  - captura de fecho que escapa.
- **Resultado sem anotação:** uma função que devolve vista toma emprestado
  de **todos** os seus parâmetros-vista. É a elisão do Rust generalizada
  de modo conservador e sem variáveis de tempo de vida.
- A única "duração" nomeada é `'static`: literais de texto e tabelas
  constantes, que podem ser guardados em qualquer lugar.
- **Empréstimo:**
  - Vale da criação da vista até o último uso. Com controle estruturado,
    isso é um intervalo calculado numa passada; se o último uso está num
    laço que a vista atravessa, o empréstimo cobre o laço inteiro.
  - Enquanto uma `&mut` de um lugar está viva, nada mais o toca.
  - Enquanto uma `&` está viva, ninguém escreve nele.
  - Campos distintos são lugares distintos; índices variáveis valem o
    array inteiro, salvo por `split_at`.
- **Por que não o verificador completo do Rust:** referência guardada é o
  que exige variáveis de tempo de vida, e aqui ela tem outro lugar: o
  índice de arena (§5.4). O que sobra, vistas e `view struct`, cobre os
  casos que um compilador e um núcleo usam:
  - analisador léxico sobre `&str`;
  - analisador sintático com `&[Token]`;
  - iteradores;
  - `split`.

  O que derrubaria isto está em §16.
- ABI: `&T` = 1 palavra; `&[T]` = 2 (endereço, comprimento); `&dyn` = 2
  (endereço, vtable).

### 5.4 Arenas e `Idx<T>`

- Uma arena é região + avanço.
- Alocar custa ~3 instruções mais o teste de limite. Liberar tudo é zerar
  o avanço e incrementar a **geração**.
- `Idx<T>` = (deslocamento: `t18`, geração: `t9`) **empacotados numa
  palavra**. É guardável, copiável e serializável: um bloco de arena pode
  ser enviado e relocado.
- `arena.get(i)` confere a geração (`rem` + comparação). Um índice velho
  dá `trap`, e nunca acesso a memória reaproveitada: é a geração de tarefa
  do núcleo (`docs/11` §F), agora para dados.
- Um índice usado na arena errada do mesmo tipo é **seguro em memória** (o
  limite é conferido), mas logicamente errado. É o mesmo nível de garantia
  de um índice de `Vec` em Rust.

### 5.5 `unsafe`

`unsafe { }` e `unsafe fn` marcam, e só elas:
- desreferenciar `*T`/`*mut T`;
- adotar memória externa (`adopt(addr, n) -> *mut [T]`, que cria uma
  alocação **nova** com tamanho, mantendo a análise de alias exata do
  `docs/13` §C.4);
- `phys_read`/`phys_write`;
- criar `Mmio`;
- `csr_write`/`csr_swap` (escrever IE = 1 quebra o contrato do núcleo:
  `docs/14` §6);
- chamar `naked fn`.

`csr_read`, `wfi` e `tlbi` não precisam de `unsafe`.

Inteiro → ponteiro continua **proibido** fora de `adopt` (T2 do `docs/13`
mantida).

### 5.6 Inicialização e o "nenhum"

- **Atribuição definida:** ler um local antes de escrever é erro (uma
  passada). O conteúdo "indefinido mas estável" do `slot` (`docs/13` §C.4)
  nunca é observável fora de `unsafe`.
- **Nicho de "nenhum" = 0 nos dois modos.**
  - O `docs/17` propôs um nicho por modo, porque no núcleo o endereço 0 é
    RAM. A regra adotada aqui é mais simples: **nenhum objeto da
    linguagem, dado ou função, ocupa o endereço 0**.
  - Em usuário, a página 0 não é mapeada **[E]**. No núcleo, o endereço 0
    é o ponto de reset (`kernel.tas`: `.org 0`, `_start`), que não é
    função de primeira classe.
  - O construtor de imagem confere a regra.
  - Ganho: memória zerada é `None` em todo enum com nicho, nos dois modos.

### 5.7 `static mut`

- Uma tarefa tem uma linha de execução e o núcleo não é reentrante (IE = 0
  nos tratadores), então ler ou escrever um `static mut` diretamente é
  **seguro**: não há corrida possível.
- O risco que sobra é o alias: uma vista de `S` viva durante uma chamada
  que também toca `S`. Como a compilação é do programa inteiro (§11.1),
  cada função tem um resumo de "estáticos tocados", calculado sobre o grafo
  de chamadas (chamada indireta conta como "todos"). A vista é recusada se
  algum chamado durante a vida dela toca o estático.
- Antes de haver resumos, criar vista de `static mut` exige `unsafe`.

## 6. Controle, funções e erros

- **Controle:**
  - `if`/`else` e `while` **[E]**;
  - `loop`;
  - `for x in a..b` e `for x in slice`;
  - `break`/`continue`, com rótulo e com valor (`loop` como expressão).
- **`match`** **[E: só literais]**. Padrões:
  - literais e faixas `a..=b`;
  - variantes com vínculo;
  - tuplas e registros;
  - `|`, `_` e guardas `if`.

  A exaustividade é conferida.
- **Funções:**
  - múltiplos resultados por tupla;
  - métodos em `impl`, com chamada uniforme;
  - `never` como tipo de retorno de quem não volta: `exit`, `trap`, laço
    servidor. Generaliza a regra L7 do `docs/11`.
- **Chamada de cauda garantida:** `become f(x)` (a palavra que o Rust
  reservou para isso). É erro de compilação se o quadro não puder ser
  reaproveitado, o que importa com pilha de 729 palavras.
- **Fechos:**
  - Sem escape: vistas, sem alocação.
  - Ponteiros de função `fn(A) -> R`: uma palavra, `jalr`.
  - Fechos que escapam: recusados, porque obrigariam alocação implícita.
- **Erros:**
  - `Result`/`Option`/`Outcome` com `?`.
  - Em `Outcome`, `?` propaga `Fail` **e** `Empty`; é o curto-circuito
    natural de três vias.
  - Pânico = `trap` (`brk`, causa −6) **[E]**: o núcleo contém, e o
    supervisor decide.
  - Sem exceções e sem desempilhamento.
- **`defer`** roda nas saídas normais do escopo (fim, `return`, `?`,
  `break`), não em `trap` nem em `exit`. Uso imediato: salvar e restaurar
  STATUS/IMASK no núcleo.

## 7. Núcleo e aplicações na mesma linguagem

### 7.1 Modos

- `#![mode(kernel)]` / `#![mode(user)]` (padrão) **[E como flag do
  módulo TIR e `--kernel`]**.
- Intrínsecas e tipos de cada modo só existem nele, nos dois sentidos
  **[E]**.
- Módulos **neutros** (sem intrínseca) servem aos dois. A biblioteca
  central é neutra, de modo que o núcleo e os programas compartilham
  arenas, `Option`, formatação e algoritmos.

### 7.2 Limite de pilha estático

- `#[stack(64)]` numa função de entrada exige que o grafo alcançável a
  partir dela seja acíclico e que a soma máxima dos quadros caiba no
  limite. Caso contrário, é erro com o caminho mais fundo.
- O núcleo hoje confere `KSTACK_LIMIT` = 64 só por teste (I9). Com isto,
  passa a conferir na compilação.
- Programas de usuário podem pedir `#[stack(729)]`.

### 7.3 `naked fn`: assembly com a granularidade de função

- `naked fn trap_entry() -> never { asm { … } }`: o corpo inteiro é
  assembly T27, montado pelo mesmo montador.
- Expressões constantes da linguagem entram nos operandos, por exemplo
  `{offset_of(Tcb, regs) + 1}(t4)`.
- O layout do TCB passa a ter **uma fonte só**: a definição do `struct`.
  Hoje são os `.equ`.
- **Asm dentro de função comum continua recusado** (`docs/17`). Ele
  quebraria a validação do TIR e a alocação de registradores, e o
  `naked fn` cobre a camada 0 (`docs/14` §1: entrada de trap, troca de
  contexto, `eret`).
- Com isto, o núcleo inteiro pode ser escrito na linguagem, com a camada 0
  isolada em funções marcadas.

### 7.4 O que fica de fora do núcleo, de propósito

- Nada de alocação no núcleo além de arenas estáticas.
- Nada de `trap` esperado: um `trap` no núcleo é pânico, e é o
  comportamento certo (parada segura, `docs/08` §4).

## 8. Concorrência e IPC

- O modelo do sistema fica **[E]**: tarefas isoladas, uma linha de
  execução por tarefa, mensagens copiadas por registradores. Por
  construção, não há corrida de dados.
- **Protocolos** são declarações:

  ```
  protocol Contador {
      call soma(n: word) -> word;   // pedido/resposta
      send zera();                  // só ida
  }
  ```

  - O compilador numera as operações e confere que cada mensagem cabe em
    a3..a9 (operação + até 6 palavras; resposta até 7).
  - Gera `Client<Contador>::soma(n) -> Outcome<word, IpcError>`, usando
    `SYS_CALL` e a paciência dada.
  - Do lado servidor, `Server<P>::recv(pac)` devolve
    `Outcome<Incoming<P>, IpcError>`. `Incoming<P>` é a união das
    operações de P, cada `call` com seu `Reply<R>` linear, mais
    `Notice(KernelNotice)`: aviso de morte ou interrupção, identificado
    pelo selo negativo (`docs/08` §3). O despacho é o sinal do selo.
- **Espera em vários canais** (`select`): depende do núcleo (objeto de
  notificação). A linguagem recebe quando ele existir.
- **Mensagens grandes:** transferência de páginas, também decisão do
  núcleo.
- **Recusado:**
  - `async`/`await`: colore funções, e a concorrência do sistema são
    tarefas;
  - fibras M:N: o escalonador é o núcleo.
- **Adiado:** corrotinas com pilha própria numa arena (biblioteca + 1
  `naked fn` de troca).

## 9. Compilação, módulos, testes

- **Constantes:** `const` e `const fn` avaliadas pelo **interpretador de
  referência do TIR**.
  - Determinístico, com teto de passos, sem intrínsecas.
  - Gera tabelas em dados inicializados.
  - `const _ = assert(...)` serve de asserção estática.
  - `size_of`/`offset_of` são embutidos.
- **Módulos:**
  - arquivo = módulo, `pub` explícito;
  - ciclos permitidos dentro de um pacote, proibidos entre pacotes;
  - pacote = diretório com manifesto.
- **Testes:** `#[test] fn`, executadas pelo harness no núcleo real com
  I1–I12 conferidos, como os testes de hoje.
- **Recusado:**
  - macros de AST: genéricos + `const fn` + protocolos na linguagem
    cobrem o uso real, e macros pioram leitura e ferramentas; reavaliar
    só com um caso que nada disso resolva;
  - RTTI;
  - REPL.

## 10. Texto

- `char` = escalar Unicode numa palavra (cabe em `t14`).
- `str` é **abstrato**: só iteração, comprimento e fatias por posição
  conhecida. Representação v1: um caractere por palavra (`docs/07` §2)
  **[E no console]**.
- O tipo de armazenamento `Text5` guarda 5 caracteres de 5 trits (243
  códigos) por palavra. Ler um é `shi` + `rem`.
- **Por que isso é decisão de arquitetura, e não detalhe:**
  - O compilador em Rust que vai ser reescrito tem ~6 100 linhas
    (210 KB).
  - Com um caractere por palavra, só o fonte ocupa ~40% da RAM padrão do
    emulador (531 441 palavras).
  - Com `Text5`, ~8%.
- Deixar `str` abstrato agora permite trocar a representação sem reescrever
  quem usa. **[H]**: a codificação canônica fica aberta até medir a
  densidade no próprio compilador (`docs/07` §2 marca [A]).

## 11. Compilação: TIR como formato objeto; ABI v2

### 11.1 Programa inteiro

- O módulo TIR binário canônico (`docs/13` §G) **é** o formato objeto.
- A geração de código acontece na ligação, sobre o programa inteiro.
  Isso habilita três coisas:
  - monomorfização entre módulos;
  - resumos de estáticos (§5.7);
  - limite de pilha entre módulos (§7.2).
- Não há biblioteca compartilhada: não há carregador dinâmico nem
  mapeamento (`docs/17`). Num sistema de imagem única, o ganho seria nulo.

### 11.2 ABI v2 (substitui o `docs/14` §2 quando implementada)

| item | regra |
|---|---|
| argumentos | achatados em palavras, em ordem, em `a1..a9`; agregado que não cabe nos que sobram vai por ponteiro para uma cópia do chamador (semântica de valor preservada) |
| resultados | achatados em `a1..a9` (até 9 palavras); acima disso, o chamador passa o destino em `a1` e o chamado constrói **no lugar** (garantido) |
| enum em registradores | discriminante primeiro, dados depois. É o formato de `sys` (`a1` = estado), logo `Outcome` sai do núcleo pronto |
| `&T` / `&[T]` / `&dyn` | 1 / 2 / 2 palavras |
| preservados | `s1..s11`, `sp` **[E]**. O código gerado pode usar `s*` salvando no prólogo, o que substitui a regra "nunca toca `s1..s10`" (S3 do `docs/14`). O núcleo continua certo porque `s1` = TCB é restaurado por quem o usa |
| `t4` | continua reservado à entrada de trap |

### 11.3 Mudanças no TIR, em ordem de necessidade

| mudança | para quê | etapa |
|---|---|---|
| até 9 resultados em função, `call` e `return` | tuplas, `Outcome`, ABI v2 | 1 |
| `BinK` `kmin`, `kmax`, `cmp3` (→ trit) | `trit`, `<=>` | 1 |
| terminadores `break n` / `continue n` | `loop`, `for`, `break` (prevista em T1 do `docs/13`) | 1 |
| interpretador de referência | oráculo e `const fn` | 1 |
| dados inicializados (tabelas, texto) | literais de texto, `const` | 2 |
| posições de fonte (tabela opcional) | trap → linha do fonte | 2 |
| `load`/`store` voláteis | MMIO de usuário; tabelas lidas pelo hardware, antes de existir otimizador | 3 |
| `adopt(addr, n)` → `ptr` de alocação nova | arena sobre memória concedida | 3 |
| tipo `fnptr` + `call_indirect` validado por assinatura | ponteiros de função, `dyn` | 4 |
| função `asm` (corpo opaco + assinatura + registradores tocados) | `naked fn` | 4 |

Os tipos do TIR continuam sendo `word` e `ptr` (mais `fnptr`):
- `bool`, `trit` e `tN` são apagados para `word` pelo front end;
- agregados são achatados em vários valores, ou ficam em `slot` quando
  têm endereço tomado.

O validador continua simples. Fatos de faixa ficam no front end, que
elimina as verificações antes de gerar TIR.

### 11.4 Backend

O que a ISA dá e o backend deve usar:
- alocação de registradores linear sobre regiões (sem controle
  irredutível, sem PHI);
- `x / 3ᵏ` → `shi`, **exato** (em binário, `sdiv` por 2ᵏ ≠ `sar`);
- tabelas de salto centradas;
- `&&`/`||` puros → `kmin`/`kmax`;
- índices de faixa sem verificação.

Cada otimização entra com medida em `tir_bench` e na suíte do núcleo
(regra do `docs/17`).

## 12. Auto-hospedagem

**Cadeia:**
- Estágio 0: o `t27r` em Rust compila o compilador escrito em T27R e
  produz o estágio 1, uma imagem T27.
- Estágio 1: roda no TENERURA emulado e compila o mesmo fonte, produzindo
  o TIR A e a imagem 2.
- A imagem 2 compila de novo e produz o TIR B e a imagem 3.

**Critérios, todos byte a byte ou palavra a palavra:**
1. `TIR(estágio 0) == A`. Duas implementações independentes da mesma
   especificação concordam. Isto exige que o `lower` seja especificado de
   forma determinística, e não só implementado assim.
2. `A == B`. É o ponto fixo.
3. `imagem 2 == imagem 3`.

Componentes a escrever na linguagem:
- front end;
- validador e codificador do TIR;
- backend: TIR → palavras T27 direto, sem texto `.tas`;
- montador, só para `naked fn`;
- construtor de imagem.

**Dependências de sistema** (não são da linguagem; ficam registradas aqui
porque decidem a etapa 5):
- memória por tarefa além da imagem: o `map` do roadmap do núcleo,
  `ESTADO.md` passo 1;
- entrada: módulos de fonte embutidos na imagem como dados;
- saída: console ou serviço de armazenamento;
- para auto-hospedagem **completa** (sem hospedeiro), um carregador e
  armazenamento: `spawn` e dispositivo de bloco, `docs/09` §H.

**[H] Custo:** desconhecido até medir. O que dá para estimar hoje:
- fonte com `Text5`: ~42 000 palavras (210 KB ÷ 5), ou ~60 000 se o
  fonte em T27R crescer 1,5× em relação ao Rust;
- o resto depende do tamanho das estruturas do compilador.

A RAM do emulador é parâmetro (`ram_half` até < 3¹⁵), não limite da
arquitetura.

## 13. Recusado, com motivo

| recurso | motivo |
|---|---|
| coletor de lixo, análise de escape que promove para heap | quebra alocação zero implícita, passos determinísticos e núcleo não preemptível |
| exceções com desempilhamento | sem tabelas de desempilhamento; o núcleo não pode depender delas |
| herança, hierarquias, promoção numérica | só alargamento sem perda (§4.1) |
| despacho múltiplo, metatabelas | nenhum uso no sistema |
| macros de AST, RTTI | §9 |
| `async`/`await`, fibras M:N | §8 |
| asm dentro de função comum | §7.3 |
| bibliotecas compartilhadas | §11.1 |
| ponto flutuante | a ISA não tem; ponto fixo 3ᵏ cobre o que o sistema precisa. Reabre se a ISA ganhar ponto flutuante |
| variáveis de tempo de vida (só `'static`) | §5.3; o que reabre está em §16 |
| atômicas de memória, `fence` | a ISA não tem; entram com um modelo de memória concorrente (`docs/07` §8) |
| byte/tryte endereçável | a ISA não tem subpalavra; `packed` e `Text5` cobrem densidade |

## 14. Diferenças em relação ao `docs/17`

O `docs/17` continua valendo onde não é contrariado aqui. O que muda:

| item do `docs/17` | lá | aqui | por quê |
|---|---|---|---|
| genéricos | adiar para depois da auto-hospedagem | **antes** dela (etapa 4) | o compilador é o maior cliente de `Option<T>`, `Result`, coleções de arena; escrevê-lo sem genéricos e reescrever é desperdício |
| asm | recusar; `extern fn` | `naked fn` na linguagem | núcleo inteiro numa linguagem, com layout de fonte única |
| nicho de nulo | por modo | 0 nos dois, por regra de layout (§5.6) | memória zerada = `None` sempre; conferível pelo construtor |
| empréstimo | `&mut` só em argumento | vistas: locais, `view struct`, resultado derivado, `'static` | cobre analisador e iteradores sem anotação |
| canais | `Cap<Proto>` | `protocol` + `Reply` linear + `main` tipado contra o manifesto | o recibo de uso único passa a ser conferido antes de rodar |
| condição | não nulo = verdadeiro | só `bool` | tipagem forte; o trit não pode virar condição em silêncio |
| inteiros | `word` + `trit` | família `tN`, arrays de índice balanceado, enum de faixas | faixas da máquina (página, VA, registrador) no tipo |
| campos de trit | adiar | `packed` | PTE e IMASK são clientes imediatos |
| resultado | `Result` com sinal | + `Outcome` de três vias, ABI = protocolo de `sys` | é a forma de todo resultado do núcleo |
| limite de pilha | "complemento recomendado" | `#[stack(N)]`, erro de compilação | I9 sai do teste e vai para o compilador |
| `static mut` | — | seguro por leitura e escrita direta; vistas por resumo | uma linha por tarefa, núcleo não reentrante |
| formato objeto | "se doer" | o TIR é o objeto; programa inteiro | determinismo e monomorfização |
| auto-hospedagem | estágio 1 == 2 | + estágio 0 (Rust) == estágio 1 | duas implementações se conferem |
| texto | `[word]` | `str` abstrato + `Text5` | ~40% da RAM só no fonte |

## 15. Implementação incremental

Cada etapa é **subconjunto da linguagem final**: nada é escrito para ser
jogado fora.

### 15.1 Reservas que toda etapa respeita

1. Palavras-chave do §3 reservadas desde a etapa 1: não podem virar nomes.
2. Nenhuma conversão implícita além do alargamento sem perda.
3. ABI v2 (§11.2) fixada antes do primeiro agregado em registradores.
4. `str` nunca exposto como `[word]`.
5. Toda leitura de MMIO ou de memória do hardware passa por intrínseca ou
   acesso volátil, mesmo antes de haver otimizador.
6. O `lower` é especificado: a numeração e a ordem do TIR fazem parte do
   contrato, por causa do critério 1 do §12.

### 15.2 Etapas

| etapa | conteúdo | critério de saída |
|---|---|---|
| **1 · núcleo tipado** | checador com tipos; `bool`, `trit`, `<=>`, Kleene, `match` exaustivo de trit, retorno vazio, `never`; múltiplos resultados e tuplas (ABI v2); `loop`/`break`/`continue`; interpretador de referência do TIR | programas existentes migrados; *fuzzing* interpretador × máquina com 0 divergências; suíte do núcleo verde |
| **2 · dados** | `tN`, `as`, registros, arrays locais, arrays de índice balanceado, enums com dados e nichos, enum de faixas, genéricos sem traits (`Option`, `Result`, `Outcome`), `const fn`, texto | o núcleo usa `Cause` e `Patience`; `pingr` sem `putc` letra a letra |
| **3 · referências e autoridade** | vistas e empréstimo, lineares, `protocol`/`Reply`, `main` tipado contra o manifesto, `unsafe`, `Mmio`, `packed`, `defer`, módulos | driver de console (`condrv.tas`) reescrito; custo medido contra o original; `irq.t27r` com `packed [trit; 27]` |
| **4 · desempenho e abstração** | alocação de registradores, eliminação de limites por faixa, traits, `dyn`, ponteiros de função, `naked fn`, `#[stack]`; no núcleo: `map` | caminhos quentes do núcleo migrados sem perder para o assembly (S1/S2 do `docs/14`) |
| **5 · auto-hospedagem** | compilador, montador e construtor de imagem em T27R | os três critérios do §12 |
| **6 · completa** | armazenamento, `spawn`/carregador | o sistema constrói e inicia a própria imagem sem o hospedeiro |

## 16. Hipóteses e o que as derrubaria

| # | escolha | derruba | confiança |
|---|---|---|---|
| H1 | vistas + índices de arena bastam; sem variáveis de tempo de vida | um trecho do compilador ou do núcleo que precise guardar referência não estática fora de `view struct`, sem que o índice resolva sem custo medido > 10% | média |
| H2 | `bool` separado de `trit` e de `word` | código de sistema cheio de `as`/`!= 0` (contar no núcleo migrado) | alta |
| H3 | enum de faixas cobre as palavras sinalizadas do núcleo | uma palavra da ABI que não se escreva como faixas disjuntas | alta |
| H4 | `Outcome` com discriminante em `a1` não custa cópias | contagem de instruções no invólucro de `recv` > 2 movimentos | média |
| H5 | `naked fn` basta para a camada 0 | algo da troca de contexto que precise de asm dentro de função com quadro | média |
| H6 | o TIR continua só com `word`/`ptr`/`fnptr` | otimização que precise de faixa no TIR (eliminação de limite entre funções) | média |
| H7 | estágio 0 == estágio 1 é alcançável | divergência que só some especificando detalhes de implementação irrelevantes | média-baixa |
| H8 | `Text5` dá ~5× de densidade sem custo relevante no analisador léxico | medição no compilador auto-hospedado | [H] |
| H9 | genéricos antes da auto-hospedagem se pagam | tamanho de texto do compilador com monomorfização > RAM útil | média |

## 17. O que não está feito

- **Nada deste documento está implementado além do que é [E] e da
  fatia do §18**: o resto das etapas 1–6 é trabalho futuro. Os custos de complexidade são
  estimativas.
- Nada é provado. A semântica do TIR não tem prova nem segunda
  implementação além dos dois geradores; o interpretador da etapa 1 é a
  segunda.
- Memória e passos para compilar o compilador no emulador: não medidos.
- Densidade de texto: não medida.
- Benefício de layout (localidade): não mensurável. O emulador não tem
  cache.

## Apêndice A — como o código fica (alvo, não compila hoje)

```
#![mode(user)]
use core::{Outcome::*, Patience};

protocol Contador {
    call soma(n: word) -> word;
    send zera();
}

fn main(srv: Server<Contador>, con: ConsoleCap) -> never {
    let mut total = 0;
    loop {
        match srv.recv(Patience::Forever) {
            Done(Incoming::soma { n, reply }) => { total += n; reply.send(total); }
            Done(Incoming::zera) => total = 0,
            Done(Incoming::Notice(_)) => {}
            Empty => {}
            Fail(_) => trap(),
        }
    }
}
```

```
#![mode(kernel)]

enum Cause: word { Fault(c: -7..=-1), Syscall = 0, Irq(l: Line in 1..=27) }

#[centered]
struct Tcb {
    caps:  [Cap; 9],
    regs:  [word; Reg],      // −13..+13; regs[0] = PC (o lugar do zr)
    state: TaskState,
    // …
}

#[stack(64)]
fn on_trap(tcb: &mut Tcb, cause: Cause) {
    match cause {                       // cmp com 0 → 2 desvios
        Cause::Fault(c) => kill(tcb, c),
        Cause::Syscall  => syscall(tcb),
        Cause::Irq(l)   => deliver(l),
    }
}
```

## 18. Estado da implementação (2026-11-17)

### 18.1 Feito: primeira fatia da etapa 1

| item | onde | teste |
|---|---|---|
| tipos `word`, `bool`, `trit` distintos; alargar `trit → word` é implícito; literal −1/0/1 serve como `trit` | `lang/check.rs` (reescrito com tipos), `parse.rs` | `tipos_errados_sao_recusados` (15 recusas) |
| `if`/`while` só aceitam `bool` (§4.2) | `check.rs` | idem; `precedencia_comparacoes_e_logica` migrado |
| `a <=> b → trit` = `cmp` | `BinK::Cmp` (TIR), `irgen`, oráculo `gen.rs` | `tres_vias_e_match_exaustivo_de_trit` |
| `&` = `kmin`, `\|` = `kmax`, sobre `trit` e sobre `bool` | `BinK::Kmin/Kmax` | `logica_de_kleene_com_kmin_e_kmax` (tabela 3×3 inteira, De Morgan) |
| `match` de `trit` exaustivo sem `_`; padrão impossível recusado | `check.rs`; `lower.rs` (o último braço vira o default) | idem |
| função sem `->` não devolve valor; `return;` só nela | `check.rs`, `lower.rs` (assinatura com 0 resultados) | `funcao_sem_valor_de_retorno` |
| as 24 palavras-chave do §3 reservadas | `lex.rs` | `let loop = …` recusado |
| **quadro por escopo**: ramos irmãos e as regiões de um laço dividem palavras | `irgen.rs` (`place`) | `ramos_irmaos_dividem_as_palavras_do_quadro`, com controle negativo: o backend antigo dá 51 palavras e o teste falha |
| *fuzzing* diferencial guiado por tipo (`word`/`bool`/`trit`, inclusive `<=>`, Kleene e `match` de trit) | `tests/tir_system.rs` | 300 programas, 0 divergências |

### 18.2 Achados

1. **O programa sintético de `tests/tir.rs` e do `tir_bench` tinha um
   padrão impossível**: `match sign(i - 5) { -1 | -2 => … }`. O `-2` nunca
   casa com um trit. A checagem nova o recusou e o `-2` saiu dos dois
   arquivos. É um exemplo do que o tipo `trit` pega.
2. **Estouro de pilha no código gerado pelo TIR.** O *fuzzer* tipado achou
   um programa (semente 213) em que os dois geradores divergiam. Não era
   semântica: o backend dava uma palavra de quadro a cada valor SSA da
   função, e o `main` grande pedia 628 palavras. Somado às chamadas, isso
   passa das 729 da pilha da tarefa (falha de escrita em −1094, a página
   abaixo da pilha). É a hipótese S2 do `docs/14` sendo falsificada em
   parte.
   - Correção: a regra de escopo do TIR (`docs/13` §C.2) já garante que
     um valor morre no fim da sua região. Agora as palavras se reaproveitam
     entre regiões irmãs e o quadro é o caminho mais fundo.
   - Efeito: `main` 628 → 55 palavras (o gerador de pilha usa 51);
     *nota 2026-11-17: reproduzido o 55/51 — programa = `Fz` de
     `tests/tir_system.rs` com `XorShift(0x7151_0000 + 213)`, `t27r` e
     `t27r --pilha`, `addi sp, sp, -N` de `fn_main`. O 628 era do gerador
     anterior à correção e não se regenera com o atual;*
     `irq_disable` do núcleo 14 → 12 (`irq.gen.tas` regenerado).
   - Instruções executadas: as mesmas.
   - Sobra: dentro de uma região em linha reta não há reaproveitamento
     (seria análise de vida). Fica para a alocação de registradores (etapa
     4).
3. **Mudança de semântica explícita**, não silenciosa:
   - `if 7 { … }` e `!0` deixaram de compilar;
   - `putc`/`putdec` não têm mais valor;
   - `return;` numa função com `-> word` é erro (antes devolvia 0).

   Programas do repositório afetados: um teste (`(0 || 7) && !0`) e o
   gerador do *fuzzer*.

> **Nota 2026-11-17 (produção) — limites de aninhamento.** Achado depois
> desta seção: 10⁴ parênteses, unários ou `if`s aninhados, ou uma soma de
> 10⁵ termos, derrubavam o `t27r` por estouro de pilha (SIGABRT): o
> analisador é recursivo e os passes seguintes recursam na árvore. Agora o
> analisador recusa com diagnóstico além de `parse::MAX_NEST = 256` níveis
> de recursão (blocos + parênteses + unários; cada bloco é uma região do
> TIR, bem abaixo de `ir::MAX_DEPTH = 1000`) ou de `parse::MAX_EXPR = 1024`
> de profundidade de árvore de expressão (cadeias `a + b + …` crescem sem
> recursão no analisador). O pior caso aceito cabe em 512 KB com
> `opt-level = 2` e pede 4–8 MB sem otimização; o `t27r` roda numa thread
> de 64 MB. Teste: `tests/lang.rs::aninhamento_fundo_e_diagnostico_nao_estouro_de_pilha`.

### 18.3 Medições (uma máquina, uma rodada; reexecutar antes de citar)

- **Suíte:** `tools/verificar-tudo.sh` → 105 testes, 0 falhas; Rust ×
  Haskell 30 000 casos, 0 divergências; provas Lean ok.
- ***Fuzzing*:** 300 programas tipados, sob o núcleo real com I1–I12.
  - Instruções de usuário: pilha 859 687, TIR 818 205 (−4,8%).
  - Não comparar com os números do `docs/14` §0.1: o gerador mudou.
- **`tir_bench 2000 7`**, comparado com o `docs/14` §0.1:

  | fase | antes | agora | Δ |
  |---|---:|---:|---:|
  | fonte → AST checada | 24,3 ms | 31,1 ms | +28%, o checador com tipos |
  | `lower` | 6,2 ms | 6,0 ms | — |
  | validação, 1 thread | 4,0 ms | 3,8 ms | — |
  | TIR → `.tas`, 1 thread | 22,0 ms | 25,3 ms | +15%, a passada de escopo |

  O texto `.tas` gerado ficou maior (320 019 → 328 019 linhas), porque a
  fonte sintética mudou (sem o `-2`) e o `match` exaustivo não tem `_`.

### 18.4 Próxima fatia (resto da etapa 1, nesta ordem)

1. Múltiplos resultados e tuplas (ABI v2, §11.2): o validador limita a 1
   (`ir.rs`).
2. `loop`/`break`/`continue`: terminador novo no TIR.
3. `never` como tipo (generaliza `exit`).
4. Interpretador de referência do TIR, e o *fuzzing* interpretador ×
   máquina.
