# TENERURA — estado real em 2026-11-17

Documento de leitura para decidir o próximo passo. Tudo abaixo é **testado**
(nada é provado formalmente, exceto a camada matemática/ALU). Fonte primária:
`~/tenerura/docs/ESTADO.md`, `docs/06`, `docs/07`, `docs/08`, `docs/09`.

---

## 0. Resposta curta

**Já dá para chamar de kernel?** Sim — é um micronúcleo mínimo que roda, e ele
tem as propriedades que definem um kernel (privilégio, isolamento, preempção,
IPC mediado, contenção de falha), não as de um programa de usuário.

**É um kernel funcional / um sistema operacional usável?** Não. Falta o que
transforma mecanismo em sistema: criar tarefas em execução, memória dinâmica,
carregador, armazenamento, arquivos. Hoje ele executa apenas programas
**embutidos na imagem de boot**.

A distinção que importa para planejar:

| | o que é | estado |
|---|---|---|
| **Plataforma** | palavra ternária, ALU, ISA T27, emulador, montador, construtor de imagem | pronto e testado |
| **Kernel** | micronúcleo em assembly T27 (1250 linhas de fonte, ~1000 palavras de código) | roda, testado, nada provado |
| **Sistema** | carregador, arquivos, memória dinâmica, criação de tarefas | **não existe** |

---

## 1. Evidência de hoje (execução real)

```
$ ./src/t27/target/release/t27 sys src/os/sys/demo2.sys 5000000 --input "ola ternura|q"

[sup] tarefa 2 falhou, causa -5: reinicio 1
[sup] tarefa 2 falhou, causa -5: reinicio 2
[sup] tarefa 2 falhou, causa -5: reinicio 3
cliente: pedidos concluidos 10, repeticoes 3
[sup] tarefa 3 saiu com codigo 0
OLA TERNURA[sup] tarefa 4 saiu com codigo 0
── parada: Idle
── passos: 305546  instruções: usuário 8718  núcleo 295969
   traps: falha 3  chamada 853  irq 3
── tarefas:
   0 condrv     bloq-recepção  geração 0
   1 super      bloq-recepção  geração 0
   2 flaky      bloq-recepção  geração 3
   3 client     morta          geração 0  saiu com código 0
   4 echo       morta          geração 0  saiu com código 0
```

Isso é um sistema, não um brinquedo: um serviço falha 3 vezes, o supervisor o
reinicia 3 vezes, o cliente repete os pedidos e conclui os 10, o driver de
console entrega a saída por interrupção — e o núcleo registra 853 chamadas.

---

## 2. O que já é kernel de verdade

| propriedade | mecanismo | evidência |
|---|---|---|
| separação de privilégio | 2 modos; opcode negativo = privilegiado | 12 testes de máquina |
| isolamento espacial | paginação em base 3; núcleo fora do espaço de usuário; W^X; página 0 vazia | invariante I8 conferido a cada saída do núcleo |
| isolamento temporal | fatia de tempo (timer); núcleo de trechos limitados | 27 cenários + caos |
| comunicação mediada | IPC síncrono por registradores; capacidades por slot; prazos; recibos | 14 classes de falha testadas |
| contenção de falha | tarefa falha → morre, dependentes acordam com erro, supervisor é avisado | teste de caos: 354 605 chamadas, 399 falhas contidas |
| recuperação | aviso de morte + reinício sob capacidade; política no supervisor | demo2 acima |
| boot de estado indefinido | tabela de boot validada; nada depende de RAM zerada | teste com RAM cheia de lixo |
| auto-verificação | 11 invariantes (I1–I11) conferidos a cada saída do núcleo | `kcheck.rs` |
| falha segura do próprio núcleo | pânico: desliga com código negativo | testado |

**Números:** 32 opcodes (8 privilegiados) · 11 chamadas de sistema · até 9
tarefas · 9 capacidades por tarefa · 64 testes · cobertura de instruções do
núcleo 99,81% · 50/50 mutantes detectados · 200 rodadas de caos · 483 468
sequências exaustivas (profundidade 5) · 46 teoremas Lean na camada de ALU
(nenhum sobre o núcleo).

**Custos medidos:** IPC ida e volta = 717 instruções de núcleo · maior trecho
não preemptível = 527 passos (8621 com reinício de tarefa) · pilha do núcleo =
24 palavras · 24 milhões de passos/s no emulador.

---

## 3. O que falta para ser "funcional" — em ordem de dependência

### A. Bloqueadores de uso real

| # | falta | por que é bloqueador | depende de |
|---|---|---|---|
| A1 | **criar/destruir tarefas em execução** | hoje o conjunto de tarefas é fixo na imagem; não há como rodar um programa que não estava no boot | memória dinâmica (A2) + carregador (A3) |
| A2 | **memória dinâmica** (quadros como objeto; mapear/desmapear) | sem isso, todo programa tem tamanho fixo e nada carrega nada | capacidades de memória (A4) |
| A3 | **carregador** (carregar programa de fora da imagem) | é o que separa "sistema" de "firmware" | A1, A2, armazenamento (A5) |
| A4 | **transferir/revogar capacidades** | **transferir: feito** na Fase 6 (`SYS_CAP_GRANT`, docs/10); falta revogar e apagar | nada (é só política no núcleo) |
| A5 | **armazenamento** (dispositivo de bloco) | sem persistência, todo boot é o mesmo boot | driver de bloco (A6) |
| A6 | **console atômico por linha** | hoje clientes diferentes entrelaçam caracteres na saída | driver de console |

### B. Robustez / escala

| # | problema | consequência |
|---|---|---|
| B1 | reinício de tarefa dentro do núcleo: 8621 passos não preemptíveis | latência de até 8621 passos para todas as outras tarefas |
| B2 | varredura O(9) em cada IPC, sem prioridades | driver pode esperar até 9 fatias; não escala além de poucas tarefas |
| B3 | supervisor é ponto único de falha | se ele morre, ninguém mais reinicia ninguém |
| B4 | `time` é global, sem canal temporal entre tarefas | uma tarefa pode inferir atividade de outra pelo relógio |
| B5 | um processador só; modelo de memória concorrente indefinido | SMP está fora de alcance hoje |
| B6 | tudo estático: sem heap de núcleo, sem filas, sem alocação pós-boot | é uma escolha (bom), mas limita o crescimento |

### C. Confiança

| # | falta | o que destrava |
|---|---|---|
| C1 | modelo abstrato + prova dos invariantes I1–I11 | hoje é amostragem ampla com mutação, não demonstração |
| C2 | segunda implementação de traps/tradução | privilégio e paginação têm uma única implementação (o emulador) |
| C3 | cache de tradução no emulador | sem ele, um `tlbi` esquecido nunca seria notado |

### D. Ferramentas — e aqui entra o que estou fazendo

| # | falta |
|---|---|
| D1 | **linguagem de sistemas para T27** (compilador) — 1250 linhas de assembly ternário não escalam para um carregador, um sistema de arquivos e um driver de bloco. **Parcial desde a Fase 7:** T27R (subconjunto de Rust) compila `fn`/`let mut`/`while`/`match`/recursão e fala com servidores em assembly; ver `docs/11` |
| D2 | depurador / traço por tarefa / observabilidade |

> **Conexão direta:** o tradutor LLVM IR → T27 que estou construindo em
> `~/tenerura-sandbox/llvm-bootstrap/` **é** o D1 em estado embrionário. Ele já
> compila C `-O0` (laços, condicionais, arrays, GEP, globais, strings, chamadas)
> e roda no núcleo real. Não é um desvio: é o pré-requisito para o núcleo
> crescer sem virar 10 mil linhas de assembly escrito à mão.

---

## 4. Leitura honesta do que existe

**A favor:** o núcleo é pequeno o bastante para ser entendido inteiro; não
aloca depois do boot; nunca segue ponteiro de usuário; toda espera tem prazo;
toda entrada tem resultado definido; a suíte tem controle negativo (mutantes).
Isso é mais disciplina do que a média dos SOs experimentais.

**Contra:** nada do núcleo é provado; o único dispositivo real é o console; o
"escalonador" é uma varredura de 9 slots; e o sistema não consegue executar um
programa que não estava na imagem de boot. Essa última é a fronteira entre
"kernel que demonstra" e "kernel que serve".

**Risco de leitura:** os documentos do projeto são rigorosos quanto a
evidência (testado ≠ provado), mas o volume de testes pode dar a impressão de
maturidade que o escopo não tem — não há um único caminho de código que
carregue código de fora.

---

## 5. Três escopos possíveis para o próximo prompt

| escopo | entrega | custo estimado | o que continua faltando |
|---|---|---|---|
| **Mínimo** (fecha o ciclo) | capacidades transferíveis + criar tarefa a partir de um `.tas` já na imagem + memória dinâmica por tarefa | médio; mexe no núcleo (1250 linhas) | persistência, arquivos, linguagem |
| **Real** (vira sistema) | o acima + driver de bloco + carregador lendo do bloco + um "shell" mínimo | alto; exige linguagem (D1) para não ser insustentável | provas, SMP |
| **Verificação** (vira confiável) | modelo abstrato dos 11 invariantes + prova; segunda implementação de traps | alto; não muda o que o sistema faz | tudo do "Real" |

Recomendação: **Mínimo** primeiro, com a linguagem (D1) em paralelo, porque
todo o resto depende de conseguir escrever mais software sem escrever
assembly à mão.

---

## 6. O que estudar antes de escrever o prompt

1. `docs/08-micronucleo.md` — o contrato do núcleo (chamadas, estados, TCB,
   capacidades). É o documento que o próximo modelo precisa respeitar.
2. `docs/09-relatorio-fase5.md` §F–H — resultados negativos, limites e a ordem
   de dependências já pensada.
3. Conceitos que decidem o desenho: **capacidade** (autoridade explícita, sem
   autoridade ambiente), **IPC síncrono com prazo** (por que toda espera tem
   teto), **preempção como requisito de contenção** (não é só justiça), e
   **invariante conferido em execução** como substituto barato de prova.
4. Uma decisão sua, que nenhum documento pode tomar: **o que "funcional"
   significa** — rodar um programa externo? persistir dados entre boots?
   múltiplos usuários? Cada resposta muda o escopo do prompt.
