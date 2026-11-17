# Fase 5 — Micronúcleo TENERURA v1

**Status (2026-11-16):** implementado em assembly T27 (`src/os/kernel/kernel.tas`,
1028 palavras de código, 1240 de dados estáticos); executa na máquina
emulada; testado (27 cenários com invariantes conferidos a cada saída do
núcleo, 200 rodadas de caos, 50 defeitos plantados todos detectados,
cobertura de instruções 99,81%). Nenhuma propriedade do núcleo está
provada formalmente: tudo abaixo é TESTADO.

## 1. O que fica no núcleo

Espaços de endereços, tarefas, fatia de tempo, IPC síncrono, encaminhamento
de interrupções, contenção de falhas (morte, aviso, reinício sob
capacidade). Fora: drivers, console (há um de depuração, sob capacidade),
nomes, arquivos, política de recuperação.

Estrutura: núcleo de EVENTOS, não preemptível, pilha única. Cada trap salva
o contexto no TCB, trata o evento até o fim e retoma alguma tarefa.
Bloquear = mudar o estado no TCB. Tudo estático: nenhuma alocação depois do
boot além de reaproveitar quadros no reinício.

O núcleo roda em endereçamento físico e **nunca segue ponteiro do usuário**:
a interface é só de registradores. Só escreve em quadros de usuário ao
(re)construir um espaço de endereços (boot e reinício) — conferido por
sonda da máquina em todo teste.

## 2. Objetos

- **Tarefa** (até 9): TCB de 81 palavras apontado pelo CENTRO; os
  deslocamentos −13..+13 guardam os registradores (deslocamento = número
  do registrador; o lugar de `zr` guarda o PC).
- **Capacidade**: 9 slots por tarefa, (tipo, objeto, selo). Tipos:
  `+1` envio / `−1` recepção num endpoint (o sinal é o lado do canal),
  `2` console de depuração, `3` tarefa (matar/consultar/reiniciar),
  `4` linha de interrupção. Concedidas no boot e **transferíveis em
  execução** por `cap_grant` (só se move o que já se tem: a origem fica
  vazia, nada é copiado nem criado).
- **Endpoint**: só um número dentro das capacidades; não existe objeto no
  núcleo nem fila — quem espera é achado por varredura dos 9 TCBs, na
  ordem de um carimbo de chegada (FIFO).

Estados (o sinal separa quem pode rodar): `+1` pronta, `0` livre, `−1`
bloqueada em envio, `−2` em recepção, `−3` aguardando resposta, `−4`
dormindo, `−5` zumbi (morta com aviso não colhido), `−9` morta.

## 3. Chamadas (`sys`; número em `a1`; ABI em `src/os/lib/abi.inc`)

Resultado em `a1`: **+1** feito · **0** nada aconteceu (ninguém do outro lado
ou prazo vencido) · **−1** erro (código em `a2`).

| nº | chamada | entrada | saída |
|---|---|---|---|
| 0 | yield | | |
| 1 | send | a2 = cap, a3..a9 = mensagem, t1 = paciência | |
| 2 | recv | a2 = cap, t1 = paciência | a2 = selo, a3..a9, t1 = recibo |
| 3 | call | como send | a3..a9 = resposta |
| 4 | reply | a2 = recibo, a3..a9 | |
| 5 | exit | a2 = código | não retorna |
| 6 | sleep | a2 = passos | |
| 7 | time | | a2 = tempo |
| 8 | putc | a2 = cap de console, a3 = caractere | |
| 9 | task | a2 = cap de tarefa, a3 = operação | consulta: a2 estado, a3 causa, a4 código, a5 geração |
| 10 | irq_ack | a2 = cap de linha | |
| 11 | cap_grant | a2 = cap de tarefa (alvo), a3 = slot de origem, a4 = slot de destino | move a capacidade; a origem fica livre |

- **Paciência** (t1), pelo sinal: negativa = espera para sempre; zero = não
  espera nem cede o processador; positiva = espera no máximo tantos passos
  (teto 3²⁰). Toda espera pode ter prazo: um servidor que não responde não
  prende o cliente.
- **Recibo**: `recv` de uma chamada entrega um recibo; `reply` só vale
  para quem recebeu aquela chamada, uma vez. Recibo 0 = não se espera
  resposta. Um servidor pode guardar vários recibos e responder depois
  (resposta adiada) — é o que permite a um driver esperar uma interrupção
  com um leitor pendente.
- **task**, pelo sinal de a3: negativo mata, zero consulta, positivo
  reinicia.
- Erros: 1 chamada inexistente, 2 capacidade inválida, 3 o servidor morreu
  ou foi reiniciado antes de responder, 4 recibo inválido, 5 slot de destino
  ocupado (a capacidade de origem NÃO é consumida nesse caso).
- **Não há autoridade ambiente**: toda chamada que toca recurso
  compartilhado nomeia uma capacidade.

**Mensagens do núcleo** chegam por `recv` com recibo 0 e SELO NEGATIVO (o
construtor de imagem reserva selos negativos ao núcleo e recusa
capacidades de envio com selo negativo): aviso de morte (a3 = id, a4 =
causa — 0 saída voluntária, < 0 falha —, a5 = tval, a6 = pc, a7 = código) e
interrupção (a3 = linha).

## 4. Falhas

| o que acontece | o que o núcleo faz |
|---|---|
| tarefa falha (qualquer causa < 0) ou sai | morre; quem aguardava resposta DELA acorda com erro 3; se tem endpoint de aviso, o supervisor recebe a mensagem (ou ela fica zumbi até ser colhida) |
| tarefa não cede o processador | perde-o ao fim da fatia (timer) |
| tarefa pede o que não pode | erro; segue viva |
| servidor não responde | o cliente decide pelo prazo |
| defeito do próprio núcleo (trap com origem no núcleo, tabela de boot inválida, falta de quadro) | PÂNICO: desliga com código negativo (parada segura; não segue com estado corrompido) |

Reinício (`task` positivo): avisa os dependentes da encarnação antiga,
zera os quadros, recarrega texto e dados da imagem original, zera
registradores, conta a geração. O texto também é recarregado.
QUANDO reiniciar é política da tarefa supervisora (`src/os/user/super.tas`:
reinicia quem falha até 3 vezes; não reinicia quem sai).

## 5. Tempo, escalonamento, interrupções

Rodízio por varredura, fatia configurável (padrão 5000 passos, mínimo 200).
Sem prioridades. Relógio modular com comparação por diferença.

Interrupção de dispositivo: o núcleo mascara a linha e a entrega ao driver
como MENSAGEM no endpoint dele (ou a deixa pendente até o próximo `recv`);
o driver reabilita com `irq_ack`. A página do dispositivo é mapeada só no
driver. Driver morto ⇒ linha mascarada. Sem tarefa pronta: `wfi` até o
próximo prazo ou interrupção; se nada mais pode acontecer, desliga (0 =
todas terminaram, 3 = restaram bloqueadas para sempre).

## 6. Invariantes conferidos a cada saída do núcleo (`src/t27/src/kcheck.rs`)

| | invariante |
|---|---|
| I1 | estados válidos; slots sem tarefa ficam livres |
| I2 | na saída: tarefa corrente pronta; PTROOT, SCRATCH, PC e os 26 registradores da máquina são os dela; modo usuário com interrupções |
| I3 | quem aguarda resposta aguarda uma tarefa VIVA; ninguém mais tem par |
| I4 | prazo armado só em quem espera; todo sono tem prazo |
| I5 | nenhum endpoint com emissor e receptor esperando ao mesmo tempo |
| I6 | carimbos de quem espera são distintos e não nulos |
| I7 | texto do núcleo e módulos de boot inalterados |
| I8 | nenhum quadro alcançável por duas tarefas; nenhuma tabela ou quadro do núcleo mapeado em usuário; nunca gravável e executável; página 0 nunca mapeada; página de dispositivo só no driver dela |
| I9 | pilha do núcleo dentro do limite (64; máximo observado 24) |
| I10 | interrupção pendente só em driver vivo |
| I11 | máscara da máquina = cópia do núcleo; linha habilitada ⇒ driver vivo |
| I12 | toda capacidade não vazia é bem formada: tipo conhecido, objeto no domínio do tipo, selo de envio não negativo, slot vazio todo zero |

## 7. Boot

O construtor (`src/t27/src/sysimage.rs`) põe o núcleo, os módulos e a
tabela de boot na imagem; valida a configuração (selos, linhas,
dispositivos, índices). O núcleo confere a tabela, constrói cada tarefa
(quadros zerados, texto RX, dados RW, pilha RW, página 0 vazia) e escalona.
Manifesto: `src/os/sys/*.sys`.

## 8. Custos medidos (`results/phase5/kernel-coverage.txt`, `bench.txt`)

- call/reply (ida e volta): 717 instruções de núcleo + 22 de usuário.
- Maior trecho não preemptível: 527 passos; com reinício de tarefa: 8621.
- Pilha do núcleo: 24 palavras.

Não há caminho rápido de IPC: é o custo de um núcleo simples, não um
limite da arquitetura.

## 9. Verificação

- `tests/kernel.rs` (28): boot, IPC, FIFO, selos, prazos, respostas
  adiadas, forja de capacidade e de recibo, 14 classes de falha, contexto
  sob milhares de preempções, lixo na RAM no boot, volta do relógio e dos
  carimbos, supervisão com reinício, driver com interrupção, pânico,
  transferência de capacidade entre tarefas.
- `tests/chaos.rs`: 200 rodadas; tarefas de chamadas aleatórias que
  também se matam e reiniciam, código aleatório; 354 605 chamadas, 399
  falhas contidas, 362 314 saídas do núcleo verificadas; as vítimas
  concluem sempre com a saída exata.
- `tests/mutants.rs`: 55 defeitos plantados no fonte do núcleo; todos
  detectados (controle negativo da própria suíte).
- `tests/exhaustive.rs`: todas as sequências de até 5 ações de um cardápio
  de 14 em 3 tarefas (483 468 sequências): nenhum invariante violado.
  Exaustivo só dentro desse limite.
- `tests/coverage.rs`: 99,81% das instruções do núcleo executadas (as 2
  restantes são laços depois de desligar).

## 10. Limites conhecidos

1. Sistema estático: 9 tarefas, capacidades concedidas no boot e
   **transferíveis** em execução (`cap_grant`), mas sem criação de tarefas,
   sem revogação e sem memória dinâmica.
2. Reinício é um trecho longo não preemptível (8621 passos) e mora no
   núcleo; o lugar natural é um carregador em espaço de usuário.
3. Varreduras O(9) em cada IPC; sem prioridades (latência de driver ≤
   9 fatias).
4. Supervisor morto ⇒ ninguém reinicia.
5. Console de usuário escreve caractere a caractere: linhas de clientes
   diferentes podem se entrelaçar.
6. `time` é global: canal temporal entre tarefas não tratado.
7. Um processador só.
