> Referência completa. Referências a `docs/NN`, Lean e Haskell apontam para o projeto
> original (completo), que não faz parte deste repositório. Comece por `PROGRAMANDO.md`.

# Fase 5 — ISA T27 e máquina de referência

**Status (2026-11-16):** especificada aqui; implementada em `src/t27/src/{word,isa,machine}.rs`;
testada (63 testes); semântica de instruções verificada por uma SEGUNDA
implementação independente em Haskell (90 000 casos, 0 divergências);
propriedades das semânticas novas provadas em Lean (`Phase5.lean`).
Nada aqui é afirmação sobre hardware físico: a máquina é um emulador no
nível de instrução.

Rótulos: **[D]** decisão desta fase · **[P]** provado em Lean · **[T]** testado ·
**[X]** verificado contra implementação independente · **[A]** aberto.

## 1. Princípios

1. A arquitetura é ternária na SEMÂNTICA; o hospedeiro binário é detalhe de
   emulação e não dita tamanhos nem formatos. **[D]**
2. Os contratos aritméticos são os da ALU auditada (docs/05): wrap
   balanceado, carry ∈ {−1,0,+1}, deslocamento à direita = arredondamento
   ao mais próximo sem empates, extensão de largura por ZERO.
3. Onde o domínio tem três casos, usa-se um trit e o SINAL decide; onde
   não tem, não se força (dois modos de privilégio, não três).
4. Decodificação simples e estrita, sem estado oculto (sem flags).

## 2. Dados

- **Palavra:** 27 trits, valor V = Σtᵢ3ⁱ ∈ [−M, M], M = (3²⁷−1)/2 = 3 812 798 742 493.
- **Memória endereçada por PALAVRA**; endereços são palavras (balanceados:
  existem endereços negativos; a RAM é centrada no endereço 0). **[D]**
- Não há unidade menor endereçável: some o conceito de alinhamento e de
  ordem de bytes dentro da palavra. Texto usa uma palavra por caractere. **[A]**
  (densidade de texto/código não medida)

Por que 27 **[D, evidência parcial]**: 27 = 3³ (o índice de trit numa palavra
é um campo de 3 trits; aninha sem resíduo — resultado C1 da Fase 3C); cabe
em `i64` no emulador; campo de instrução folgado. Não foi comparado
experimentalmente com 18, 24 ou 81 trits.

## 3. Registradores e formato de instrução

27 registradores, campo de 3 trits (−13..+13). O SINAL do número é a classe
de preservação da ABI:

| nº | nome | papel |
|---|---|---|
| 0 | `zr` | sempre zero |
| +1..+9 | `a1`..`a9` | argumentos, resultados, mensagem IPC (voláteis) |
| +10..+13 | `t1`..`t4` | temporários (voláteis) |
| −1..−11 | `s1`..`s11` | preservados por quem é chamado |
| −12 | `lr` | endereço de retorno |
| −13 | `sp` | pilha (cresce para endereços menores) |

Toda instrução ocupa uma palavra; campos em posições fixas:

```
trits   26..22   21..19   18..16   15..13   12..0
R       op       a        b        c        (zero)
I       op       a        b        imm16 (15..0)
J       op       a        imm19 (18..0)
N       op       (zero)
```

- Campos são números balanceados. **Imediatos não têm extensão de sinal**:
  estender por zero preserva o valor (Fase 4A). Qualquer constante de 27
  trits é `lui` (11 trits altos) + `addi` (16 baixos) com os campos lidos
  direto dos dígitos, sem o ajuste de empréstimo das ISAs binárias. **[P]** `split_hi_range` **[T]**
- **Decodificação estrita:** campo não usado ≠ 0 ⇒ instrução ilegal; cada
  instrução válida tem uma única codificação. **[T][X]**
- **Opcode 0 é ilegal para sempre:** memória zerada não executa. **[T]**
- **Opcode negativo ⇔ instrução privilegiada.**

## 4. Instruções

| op | mnem. | fmt | efeito |
|---|---|---|---|
| 1 | `add a,b,c` | R | a ← wrap(b + c) |
| 2 | `sub` | R | a ← wrap(b − c) |
| 3 | `addc` | R | a ← carry de b + c ∈ {−1,0,+1} |
| 4 | `mul` | R | a ← palavra baixa de b·c |
| 5 | `mulh` | R | a ← palavra alta de b·c (b·c = lo + hi·3²⁷) |
| 6 | `div` | R | a ← quociente centrado (§4.1) |
| 7 | `rem` | R | a ← resto centrado |
| 8 | `cmp` | R | a ← sinal(b − c), exato |
| 9 | `sh` | R | a ← b deslocado por c (§4.2) |
| 10 | `kmin` | R | a ← mínimo trit a trit (∧ de Kleene) |
| 11 | `kmax` | R | a ← máximo trit a trit (∨ de Kleene) |
| 12 | `addi a,b,imm` | I | a ← wrap(b + imm) |
| 13 | `shi a,b,imm` | I | a ← b deslocado por imm |
| 14 | `lui a,imm` | I | a ← imm·3¹⁶ (b = zr; \|imm\| ≤ (3¹¹−1)/2) |
| 15 | `ld a,imm(b)` | I | a ← mem[b + imm] |
| 16 | `st a,imm(b)` | I | mem[b + imm] ← a |
| 17–22 | `beq bne blt bge bgt ble a,b,alvo` | I | desvia se cmp(a,b) está na classe; alvo = PC + imm |
| 23 | `jal a,alvo` | J | a ← PC+1; PC ← PC + imm |
| 24 | `jalr a,imm(b)` | I | t ← b + imm; a ← PC+1; PC ← t |
| 25 | `sys` | N | trap, causa 0 |
| 26 | `brk` | N | trap, causa −6 |
| −1 | `eret` | N | retorno de trap |
| −2 | `wfi` | N | espera interrupção pendente e habilitada na máscara |
| −3 | `csrr a,csr` | I | a ← csr |
| −4 | `csrw csr,b` | I | csr ← b |
| −5 | `csrx a,csr,b` | I | troca atômica |
| −6 | `tlbi` | N | invalida traduções em cache |

Sem registrador de flags **[D]**: `cmp` entrega um trit num registrador e os
desvios comparam dois registradores (com `zr`, testam o sinal). O carry sai
por `addc`. A negação é exata em toda a janela (não existe o "menor
negativo" assimétrico), logo `sub` ≡ `add` com negação, inclusive no carry.

### 4.1 Divisão centrada (semântica NOVA desta fase) **[D]**

`div`: q = inteiro mais próximo de b/c; empate (só com divisor par) → o
mais próximo de zero. `rem`: r = b − q·c, |2r| ≤ |c|. Total: c = 0 ⇒ q = 0,
r = b (a identidade b = q·c + r vale sempre; não há trap).

Por quê: é a única divisão coerente com o deslocamento — dividir por 3ᵏ
**é** descartar k trits (`shi` negativo). Em binário, `sar` e `idiv`
divergem; aqui não. **[P]** `div_pow3_drops_low_digits`, `cdiv_unique`,
`cdiv_exists`, `cdiv_tie_toward_zero`, `cdiv_neg` · **[T][X]**.
Limite: o Lean prova a especificação e um algoritmo de referência; a
correspondência do emulador com ela é por teste, não por prova.

### 4.2 Deslocamento com sinal **[D]**

Um só deslocamento; o sinal do contador dá o sentido: k > 0 sobe dígitos
(×3ᵏ, os de topo saem), k < 0 descarta |k| trits baixos (arredonda ao mais
próximo), |k| ≥ 27 dá zero. Substitui os seis opcodes SLL/SRL/SRA×{reg,imm}
de uma ISA binária por dois (`sh`, `shi`).

## 5. Privilégio, traps e interrupções

Dois modos: núcleo e usuário. STATUS (CSR 1) = MODE + 3·IE + 9·PMODE + 27·PIE,
cada um ∈ {0,1}; outras codificações são instrução ilegal.

Trap: PMODE←MODE, PIE←IE, MODE←núcleo, IE←0, EPC←PC, CAUSE, TVAL, PC←TVEC.
Exceções são precisas (a instrução faltosa não deixa efeito).

**O sinal de CAUSE classifica o trap** (o núcleo despacha com um desvio de
três vias): negativo = falha, zero = chamada de sistema, positivo = número
da linha de interrupção.

| causa | significado | TVAL |
|---|---|---|
| 0 | `sys` | 0 |
| −1 | instrução ilegal | a palavra |
| −2 | instrução privilegiada em modo usuário | a palavra |
| −3 / −4 / −5 | falha de busca / leitura / escrita | endereço virtual |
| −6 | `brk` | 0 |
| −7 | erro de barramento (endereço físico inexistente) | endereço |
| +1 | timer | — |
| +2 | console (entrada disponível) | — |

CSRs: 1 STATUS, 2 TVEC, 3 EPC, 4 CAUSE, 5 TVAL, 6 SCRATCH, 7 PTROOT, 8 IMASK
(um trit 0/+1 por linha), 9 IPEND (leitura), 10 IRQN (leitura: menor linha
pendente e habilitada), 11 TIME (leitura), 12 TIMECMP.

**Tempo:** TIME conta passos de máquina e é MODULAR (27 trits). O timer
está pendente ⟺ (TIME − TIMECMP), reduzido à janela, é ≥ 0. Comparar pela
diferença continua correto quando o contador dá a volta, desde que os
prazos fiquem a menos de meia janela. **[P]** `serial_difference_exact` **[T]**
(Defeito de projeto encontrado e corrigido: a primeira versão comparava
direto — docs/09, achado F2.)

`wfi` conclui quando há linha pendente e habilitada em IMASK, mesmo com IE = 0.

## 6. Memória e proteção

Em modo núcleo o endereçamento é FÍSICO (sem tradução): o núcleo não é
mapeado nos espaços de usuário. Em modo usuário todo acesso é traduzido.

- **Página = 3⁶ = 729 palavras, CENTRADA**: a página p cobre
  [729p − 364, 729p + 364]. Número da página = endereço arredondado ao
  múltiplo de 729 mais próximo; deslocamento ∈ [−364, +364]. A decomposição
  existe e é única. **[P]** `page_split_exists`, `page_split_unique`
- A página 0 é centrada no endereço 0: deixá-la sem mapear protege o
  ponteiro nulo nos DOIS sentidos.
- Endereço virtual canônico: 18 trits (os 9 altos zero). Tabela de 2
  níveis, índices balanceados de 6 trits; cada tabela ocupa uma página e é
  indexada a partir do CENTRO.
- **PTE = centro do quadro + flags** (6 trits baixos): trit 0 = +1 folha,
  −1 ponteiro de tabela, 0 ausente; trit 1 = +1 gravável; trit 2 = +1
  executável; trits 3–5 zero. Palavra zero = não mapeado.
- Base 3 pura: separar página/deslocamento é só fiação num hardware
  ternário. Medido (results/phase5/bench.txt): no modelo de contagem, 0
  células somadoras contra ~432 para páginas de 1000 ou 1024 palavras; no
  emulador binário custa 2,1 ns contra 0,5 ns de uma máscara — preço da
  emulação, não da arquitetura. Fecha a pendência "custo de endereçamento
  em radix mista" da Fase 3C. **[T]** (modelo de contagem, não síntese)

## 7. Dispositivos, reset, boot

Dispositivos são páginas de memória fora da RAM (um por página, para poder
conceder cada um a um driver): console em 3¹⁵ (+0 TX, +1 RX, +2 STATUS),
energia em 3¹⁵+729 (escrita desliga com o código escrito).

Reset: PC = 0, modo núcleo, IE = 0, CSRs zero, `a1` = meia-extensão da RAM.
Os demais registradores e a RAM não têm valor garantido (os testes rodam
com lixo). O carregador do emulador faz o papel de firmware.

## 8. Fora da ISA (e por quê)

Flags (estado oculto); ponto flutuante; multiprocessador e modelo de
memória concorrente (**[A]** — precisa ser definido antes de qualquer SMP);
endereçamento de subpalavra; terceiro modo de privilégio (nenhum uso
demonstrado); instruções de campo de trits (extração se faz com dois
deslocamentos). `tlbi` está definida, mas o emulador não tem cache de
tradução — um núcleo que esquecesse o `tlbi` NÃO seria apanhado. **[A]**

## 9. Montador (`src/t27/src/asm.rs`)

`rótulo: mnemônico operandos ; comentário`. Seções `.text`/`.data` (dados
começam na página seguinte ao texto). Diretivas: `.org .equ .word .zero
.fill .string .stringz .page .entry .assert .include .macro/.endm`. Rótulos
locais `1:` / `1b` / `1f`. Pseudo: `li la mov neg sgn nop j call ret jr push
pop bz bnz bn bp bnn bnp`. Em expressões `/` e `%` são a divisão centrada
da máquina. Ferramenta: `t27 asm|run|sys` (`src/t27/src/bin/t27.rs`).

## 10. O que o ternário balanceado mudou na ISA (demonstrado)

| efeito | evidência |
|---|---|
| sem extensão de sinal; constantes por divisão pura de dígitos | [P] `split_hi_range`, [T] `la_splits_digits_exactly` |
| um deslocamento com sinal em vez de três famílias | [T][X] |
| `div` por 3ᵏ ≡ deslocamento | [P] `div_pow3_drops_low_digits`, [T] |
| negação nunca estoura; `sub` ≡ `add`∘`neg` também no carry | [T] diferencial contra a ALU auditada |
| páginas centradas, decomposição única, guarda de nulo simétrica | [P] `page_split_unique`, [T] |
| classes de três casos por sinal (causa de trap, `cmp`) | [T] |

O que NÃO foi demonstrado: qualquer vantagem de desempenho, área ou energia
sobre uma máquina binária.

## 11. Evidência e limites

- `tests/word_diff.rs`: palavra do emulador × ALU auditada (fronteiras ×
  fronteiras + 40 000 pares). `tests/machine.rs`: execução, traps,
  tradução, interrupções (12 testes).
- `tools/isa-crosscheck.sh`: emulador × `experiments/phase5/T27Ref.hs`
  (listas de dígitos, biblioteca da Fase 1): 3 sementes × 30 000 casos,
  0 divergências; controle negativo acusa 1 valor adulterado
  (`results/phase5/isa-crosscheck.log`). Cobre as instruções de usuário;
  CSRs, traps e tradução têm UMA implementação só (o emulador).
- Decisões sem evidência suficiente: largura 27; endereçamento por
  palavra; página de 729; espaço virtual de 18 trits; 27 registradores;
  instrução de largura fixa. Ver docs/06 §7.
