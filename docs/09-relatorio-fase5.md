# TENERURA — Relatório da Fase 5: da ALU a um sistema operacional que roda

Data: 2026-11-16 · sessão autônoma · nada foi commitado (o repositório não
é git na raiz). Documentos: 06 (maturidade), 07 (ISA), 08 (núcleo),
`ESTADO.md` (por onde retomar).

## A. Resumo

Antes: semântica, codificações e uma ALU de referência. Não havia ISA,
máquina, montador nem núcleo.

Agora, executando e testado:

1. **ISA T27** (27 trits, endereçada por palavra, balanceada) e **emulador**
   determinístico com privilégio, traps, paginação em base 3, timer e
   dispositivos.
2. **Montador** e ferramenta `t27`.
3. **Micronúcleo v1 em assembly ternário** (1028 palavras): tarefas
   isoladas, preempção, IPC por registradores com prazos e recibos,
   capacidades, contenção de falhas, aviso de morte, reinício, interrupções
   entregues a drivers em espaço de usuário.
4. **Espaço de usuário de demonstração**: driver de console, supervisor,
   serviço que falha e é reiniciado, cliente que se recupera, eco por
   interrupção.

Objetivo operacional da tarefa (duas cargas independentes que se comunicam;
a falha de uma não compromete a outra nem o núcleo): **atingido e testado**,
com recuperação além do pedido. Não provado formalmente.

## B. Achados (defeitos e correções)

| # | achado | como apareceu | o que foi feito |
|---|---|---|---|
| **L1** | **As provas Lean das Fases 3A e 4A nunca foram verificadas.** `lake build` construía só o executável (`Main` importa apenas `Basic`); `Phase3.lean` tinha 20 erros e `Phase4A.lean` 34. Os relatórios 04, 05 e a auditoria 4A diziam "provado, build limpo". | ao construir o alvo da biblioteca para a prova nova | provas refeitas até compilar; enunciados mantidos, exceto `addC_spec` (quantificava a palavra como `Int`, mal tipado); o alvo padrão do `lake build` agora inclui a biblioteca; originais em `results/phase5/lean-original/`; log do build quebrado em `results/phase5/lean-build-antes-da-correcao.log`; notas datadas nos documentos antigos |
| F1 | console de depuração era autoridade ambiente: qualquer tarefa embaralhava a saída das outras | teste de caos, rodada 18 | `putc` passou a exigir capacidade |
| F2 | relógio de 27 trits comparado direto: um `sleep` longo levaria o tempo à volta e a fatia de tempo deixaria de funcionar (negação de serviço) | revisão de projeto ao preparar o caos | relógio modular com comparação por diferença na ISA e no núcleo; teto nos prazos; provado (`serial_difference_exact`) e testado atravessando a volta |
| F3 | "responde ao último chamador" não permite a um driver guardar um leitor enquanto espera a interrupção | ao desenhar o driver | recibos explícitos; resposta adiada; prazos em toda espera |
| F4 | linha de interrupção habilitada na máquina e desligada na cópia do núcleo virava tempestade de interrupções | teste de cobertura (linha forçada pelo arnês) | `irq_disable` sempre reescreve a máscara |
| F5 | origem de avisos de morte/interrupção era falsificável por quem tivesse capacidade de envio com o mesmo selo | revisão | selos negativos reservados ao núcleo, exigido pelo construtor de imagem |
| T1 | cenários de teste fracos: 8 defeitos plantados sobreviveram na primeira passada (fila de receptores, zumbi, driver morto, volta do relógio caindo no boot…) | testes de mutação | cenários novos para 7; o 8º era código redundante do núcleo, removido |
| T2 | 4 "divergências" Rust×Haskell | verificação cruzada | artefato do gerador (a instrução caía na célula de memória do caso); corrigido — 0 divergências reais |

## C. O que mudou no repositório

Novo: `src/t27/` (emulador, montador, construtor de imagem, verificador de
invariantes, testes, medições), `src/os/` (núcleo, biblioteca e programas
de usuário, manifestos), `experiments/phase5/T27Ref.hs`,
`src/lean/TeneruraProofs/Phase5.lean`, `tools/t27-test.sh`,
`tools/isa-crosscheck.sh`, `docs/06..09`, `docs/ESTADO.md`, `results/phase5/`.

Alterado de fases anteriores (só o necessário): `Phase3.lean`,
`Phase4A.lean` (provas), `TeneruraProofs.lean` (importa Phase5),
`lakefile.toml` (alvo padrão), e notas datadas em `docs/04`, `docs/05`,
`results/phase4a/auditoria.md`. `src/rust`, `src/haskell` e os resultados
antigos não foram tocados (47 testes da Fase 4A reexecutados: verdes).

## D. Testes (todos executados; `results/phase5/t27-tests.log`)

`tools/t27-test.sh` → 64 testes, 0 falhas (`tools/verificar-tudo.sh` roda
isto, a verificação cruzada e as provas num comando só):

| arquivo | nº | o que cobre |
|---|---|---|
| unitários (`src/`) | 17 | palavra, codificação, montador; decodificação rápida = de referência (400 000 palavras) |
| `word_diff.rs` | 3 | palavra do emulador × ALU auditada |
| `machine.rs` | 12 | máquina nua: traps precisos, tradução, timer, CSRs |
| `kernel.rs` | 27 | cenários de sistema com invariantes a cada saída do núcleo |
| `chaos.rs` | 1 | 200 rodadas hostis: 354 605 chamadas, 399 falhas contidas |
| `mutants.rs` | 2 | 50 defeitos plantados, 50 detectados; núcleo real passa em todos os detectores |
| `coverage.rs` | 1 | cobertura do núcleo 99,81%; custos |
| `exhaustive.rs` | 1 | TODAS as sequências de até 4 ações (5 com `T27_DEPTH=5`) de um cardápio de 14, em 3 tarefas, no núcleo real: 483 468 sequências na profundidade 5, 493 759 saídas do núcleo conferidas, 2 975 estados abstratos distintos, 0 violações (`results/phase5/exhaustive.txt`). Controle negativo: com um defeito plantado acha o contraexemplo `[Yield, Recv, Call, Kill]` |

Fora do cargo: `tools/isa-crosscheck.sh` (90 000 casos, 0 divergências,
controle negativo ok) e `lake build` (46 teoremas, sem `sorry`, só axiomas
padrão — `results/phase5/lean-build.log`).

Sementes fixas (xorshift64*, derivadas de `0x5EED_2026_1116`); execução
determinística (mesmo número de passos com ou sem lixo na RAM).

## E. Medições (`results/phase5/bench.txt`; uma máquina, uma rodada)

- **Representação da palavra no emulador** (ns/op): soma 1,25 (`i64`) ·
  14,0 (2 planos de bits) · 66,6 (`[i8;27]`) · 84,4 (`Vec<i8>`);
  multiplicação 30,2 × 673,6; palavra→índice 0,23 · 13,8 · 16,1; lógica
  trit a trit 91,3 (`i64`) × 0,42 (planos). Na mistura real (demo 2: 38%
  memória, 29% soma, 23% desvio) o custo por instrução é 0,88 · 23,6 ·
  42,4 ns. Decisão: `i64` no emulador; a ALU `Vec<i8>` continua como oráculo.
  Fecha a pendência "ALU × representação" da Fase 4.
- **Endereçamento**: base 3 pura = 0 células somadoras num modelo de
  hardware ternário contra ~432 para página de 1000/1024; no emulador, 2,1
  contra 0,5 ns. Fecha a pendência da Fase 3C (com a ressalva: modelo de
  contagem, não circuito).
- **Emulador**: 24 milhões de passos/s; decodificação 22,7 ns no
  microteste.
- **Núcleo**: IPC ida e volta 717 instruções de núcleo; maior trecho não
  preemptível 527 passos (8621 com reinício); pilha 24 palavras.

## F. Resultados negativos

1. Nenhuma vantagem do ternário em custo, energia ou desempenho foi
   demonstrada — nem procurada com meios capazes de mostrá-la.
2. Não se achou uso para um terceiro modo de privilégio.
3. Os planos de bits ganham 200× na lógica trit a trit e perdem 27× no que
   o sistema realmente executa.
4. A otimização da decodificação rendeu só ~20% (19,9 → 24,0 M passos/s).
5. "Voltar a receber abandona o chamador pendente" (v0) teve de ser
   descartado: impede resposta adiada.
6. Direitos de endpoint como um trit "0 = ambos" não encaixaram; ficaram
   dois tipos de capacidade.
7. O emulador paga ~4,5× para separar página em base 3 em vez de máscara.

## G. Limitações (o que NÃO está feito ou verificado)

- Nada do núcleo é provado; contenção e isolamento são testados por
  amostragem (ampla, com mutação como controle), não demonstrados.
- Privilégio, traps e tradução têm uma única implementação (o emulador).
  O emulador não tem cache de tradução: `tlbi` esquecido não seria notado.
- O Lean prova a especificação da divisão centrada e um algoritmo de
  referência, não o código do emulador.
- Sistema estático (docs/08 §10): sem criação de tarefas, memória
  dinâmica, transferência ou revogação de capacidades; um processador.
- Só um dispositivo real (console); sem armazenamento.
- Medições de uma máquina, uma rodada.

## H. Próximos passos, por dependência

1. Exploração exaustiva mais funda (podar por estado abstrato já visitado)
   e, depois, modelo abstrato + prova dos invariantes (I1–I11 na Fase 5; I12 desde a Fase 6). O primeiro
   degrau — exaustão limitada sobre o núcleo real — está feito (`exhaustive.rs`).
2. Cache de tradução no emulador (para apanhar `tlbi` ausente) e segunda
   implementação de traps/tradução.
3. Sistema dinâmico: quadros como objetos, mapear/desmapear, criar tarefa,
   transferir e revogar capacidades; mover o carregador (e o reinício) para
   espaço de usuário.
   **Nota de 2026-11-17:** a transferência de capacidades foi feita na
   Fase 6 (`SYS_CAP_GRANT`, docs/10); revogação, quadros e criar tarefa
   continuam pendentes.
4. Linguagem de sistemas mínima para T27.
   **Nota de 2026-11-17:** feita na Fase 7 como **T27R** (subconjunto de
   Rust, front end próprio → `.tas`, `docs/11`); `ping.t27r` já fala com o
   `pong.tas` em assembly. Faltam strings, `enum`/`struct` e retorno vazio.
5. Memória compartilhada para mensagens grandes; console atômico por linha.
6. Dispositivo de bloco e um serviço de armazenamento.

## I. Como reproduzir

```
tools/verificar-tudo.sh                        # testes + Rust × Haskell + provas
cd src/t27 && nix-shell ../../shell-rust.nix --run \
  'cargo run --release --bin t27 -- sys ../os/sys/demo2.sys 20000000 --input "ola ternura|q"'
cd src/t27 && nix-shell ../../shell-rust.nix --run 'cargo run --release --example bench'
```
