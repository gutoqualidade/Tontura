# TENERURA — Fase 6: autoridade móvel

Data: 2026-11-17 · sessão autônoma, executada **em cópia**
(`~/tenerura-sandbox/`); `~/tenerura/` não foi tocado (conferido por marca
de tempo e `diff -rq`). Documentos vizinhos: `08` (núcleo), `09` (Fase 5),
`ESTADO.md`.

## A. O que foi verificado, e não apenas lido

Antes de decidir qualquer coisa, o estado foi reexecutado numa cópia limpa:

| afirmação de relatório | verificação de hoje |
|---|---|
| "64 testes verdes" | `tools/verificar-tudo.sh` → 64/64, saída 0 |
| "30 000 casos Rust × Haskell, 0 divergências" | reexecutado: 0 divergências (controle negativo acusa 1) |
| "provas Lean ok" | `lake build` do alvo da biblioteca: ok, só axiomas padrão |
| "boot mínimo existe" | medido: **9 865** instruções de núcleo para o núcleo real + 1 tarefa de 2 instruções |
| "imagem do sistema" | medida: **7 341** palavras (`demo2`), núcleo 2 268 |
| "24,0 M passos/s" | `results/phase5/bench.txt`, mesma máquina |

Três coisas que os relatórios não diziam:

1. A tabela de "estado do conhecimento" estava desatualizada: os benchmarks
   de representação e o tamanho de imagem **já** existiam (Fase 5).
2. O fator 5,05× do cálculo "100 MB → 505 MB" não tem fonte no projeto.
3. O núcleo tinha uma limitação que nenhum documento listava como
   bloqueante, mas que trava todo o resto: **toda autoridade é decidida no
   boot**. Nenhuma tarefa pode ganhar um direito depois de criada.

Classificação honesta do que existe:

| | o que é |
|---|---|
| implementado | ISA, emulador, montador, construtor de imagem, núcleo, 5 programas de usuário |
| demonstrado | duas cargas independentes conversando; falha contida; serviço reiniciado 3×; driver por interrupção |
| sistematicamente testado | invariantes I1–I12 a cada saída do núcleo, 200 rodadas de caos, exaustão de 483 468 sequências, 55 mutantes |
| formalmente provado | só a ALU e a aritmética (46 teoremas Lean). **Nada do núcleo.** |
| não verificado | desempenho contra um SO binário equivalente; hardware ternário nativo |

## B. Direção arquitetural: por que micronúcleo, e quando reconsiderar

O prompt pede para não tratar o micronúcleo como compromisso irreversível.
A comparação, com os números que existem:

| arquitetura | ganho esperado aqui | custo, medido ou dedutível |
|---|---|---|
| monolítico | corta a troca de espaço de endereços e o IPC | mata a contenção de falhas — a única propriedade duramente testada (399 falhas contidas em 354 605 chamadas; reinício de serviço em `demo2`); o núcleo passaria de 2 314 para ~6 000 palavras e nenhum teorema o cobre |
| monolítico modular | organização sem perda de desempenho | sem domínios de proteção por módulo, "módulo" é só convenção: exige carregador e memória dinâmica antes de existir |
| micronúcleo (atual) | isolamento real, falha contida, reinício, superfície pequena | paga IPC em cada interação: 717 instruções de núcleo por ida-e-volta |
| híbrido (núcleo + anéis de memória compartilhada) | corta o custo por interação | precisa de memória compartilhada, que hoje não existe (não há chamada de mapeamento) |
| exocerne / unikernel | — | contraria o objetivo de isolamento; exige tudo o que falta e mais |

**Decisão: manter o micronúcleo, com uma ressalva quantitativa.** O imposto
do micronúcleo é *por interação*, não por instrução executada:

```
demo2: 853 traps / 8 718 instruções de usuário  → 96,9 % do total é núcleo
       (carga patológica: 10 pedidos, cada um uma ida-e-volta)
carga de cálculo: ~0 traps por instrução de usuário → imposto → 0
```

**Critério de revisão (falsificável):** se, numa carga realista, a fração de
instruções de núcleo continuar acima de ~90 % *depois* de descontado o boot,
o gargalo é a interação e não o cálculo; nesse caso o caminho é o híbrido —
um anel de memória compartilhada para o par cliente/servidor quente, com o
IPC por registradores como caminho lento. Hoje essa condição não está
demonstrada (a carga medida é justamente a patológica), e o híbrido exige
memória dinâmica que ainda não existe.

O que o exame revelou é outra coisa: o limite arquitetural real **não** é a
divisão núcleo/servidores, é a **estaticidade da autoridade**. Todo direito
nasce no boot e morre com a tarefa. Isso trava, em cadeia:

```
autoridade móvel (feito agora) ─┐
memória dinâmica (quadros, mapa)├→ criar tarefa → carregador em espaço de usuário
proveniência de concessões ─────┘        └→ revogação
```

## C. O marco escolhido, e por que ele e não outro

Três candidatos, na ordem da cadeia acima:

| candidato | por que não agora |
|---|---|
| memória dinâmica (alocador de quadros + `map`) | maior ganho potencial, mas exige alocador no núcleo: novas falhas (esgotamento), novas invariantes (dono do quadro), novo caminho de pânico. É o próximo, não este. |
| criar tarefa | depende de memória dinâmica **e** de poder entregar capacidades ao novo dono. Sem a segunda, a tarefa nasceria sem direito nenhum. |
| revogação | exige registrar a proveniência de cada concessão. Sem concessões, não há o que revogar. |
| **autoridade móvel** | **46 palavras, nenhum objeto novo, nenhuma invariante nova de estado — e destrava os outros três.** |

Escolhido: `SYS_CAP_GRANT`. Mover, não copiar: quem já tem um direito pode
passá-lo adiante e deixa de tê-lo. Assim não existe caminho para forjar nem
para duplicar autoridade, e a revogação futura tem uma estrutura natural (a
cadeia de concessões) em vez de contagem de referências.

## D. A chamada

```
a1 = 11 (SYS_CAP_GRANT)
a2 = capacidade de TAREFA que nomeia o alvo      (só ela nomeia outro dono)
a3 = slot de origem, nesta tarefa
a4 = slot de destino, no alvo
→ a1 = +1 feito (a origem fica vazia) | −1 erro (código em a2)
```

O alvo é nomeado por **capacidade**, não por índice: não existe "PID
ambiente" — para dar algo a alguém é preciso já ter autoridade sobre esse
alguém.

| situação | resultado |
|---|---|
| a2 não é capacidade válida | `E_BADCAP` (2) |
| a2 é capacidade, mas não de tarefa | `E_BADCAP` |
| objeto da capacidade de tarefa fora de `0..ntasks` | `E_BADCAP` |
| origem vazia | `E_BADCAP` |
| destino negativo ou ≥ 9 | `E_BADCAP` |
| destino ocupado | `E_NOSLOT` (5) — **nada é sobrescrito e a origem não é consumida** |
| tudo certo | `+1`, capacidade movida, origem zerada |

Custo: núcleo de 2 268 → **2 314 palavras** (+45 de texto, +1 de tabela).
IPC, pilha e maior trecho não preemptível **não mudaram** (717 / 24 / 527).

## E. O que os testes pegaram

1. **Defeito real no núcleo:** a origem vazia não era recusada. O
   `cap_lookup` devolve o endereço do slot, não a capacidade; sem conferir
   `CAP_KIND`, a chamada "movia" o vazio para o destino e devolvia sucesso —
   um direito que não existe passando por transferência. Pego pela borda
   `origem vazia` (saída 74).
2. **Suposição errada minha no teste:** esperei `Stop::Idle` no fim; um
   sistema cujas tarefas terminam devolve `Halted(0)`. Corrigido no cenário,
   não no núcleo.
3. **O teste de mutação obrigou o desenho:** o trecho
   `bne t1, t2, bad_cap` já existia no núcleo (envio, `putc`, tarefa) e os
   mutantes casam por texto com ocorrência única. A conferência de tipo
   virou `beq t1, t2, 1f` + `j bad_cap`, que é único. Efeito colateral bom:
   os 5 mutantes novos de `cap_grant` são detectáveis sem ambiguidade.

## F. Verificação acrescentada

- Cenário `sc_cap_grant` (detector "autoridade móvel"), em duas partes:
  1. o doador passa o console a um recebedor que **nasceu sem console**;
     o recebedor escreve e conclui 42; a origem do doador fica vazia;
  2. bordas na própria tarefa: origem vazia, destino ocupado,
     autotransferência no mesmo slot, `a2` que não é capacidade de tarefa,
     destino ≥ 9 e destino negativo.
- **I12 novo no verificador de invariantes**: toda capacidade não vazia é
  bem formada (tipo conhecido, objeto no domínio do tipo, selo de envio não
  negativo, slot vazio todo zero). Controle negativo feito à mão: um núcleo
  alterado para gravar tipo 9 foi acusado com
  `I12: recebedor slot 0 com tipo desconhecido 9`.
- **5 mutantes novos** (55 no total), todos detectados:
  alvo que não é tarefa · origem vazia aceita · destino ocupado
  sobrescrito · limites do destino não conferidos · **cópia em vez de
  movimento**.
- Cobertura de instruções: **99,81 %** (as 2 restantes são os laços depois
  de desligar). Suíte: **65 testes**, 0 falhas; `demo2` com o mesmo número
  de passos de antes (305 546) — sem regressão.

## G. Limites que esta fase cria ou expõe

1. **Não há como apagar uma capacidade.** Um doador malicioso pode encher
   os 9 slots de outra tarefa com `cap_grant` e negar-lhe slots para
   sempre. O remédio é `cap_delete`/`cap_revoke`, que precisa de
   proveniência — o próximo passo natural depois da memória dinâmica.
2. **O alvo não é consultado.** Receber capacidade não cria obrigação, mas
   ocupa slot. Aceitável enquanto não houver revogação; a alternativa
   (destino escolhido pelo núcleo) esconde do doador onde a capacidade caiu.
3. **Concessão não atravessa reinício:** quem reinicia volta com as
   capacidades que estavam no seu TCB (as recebidas incluídas), não com as
   do boot.
4. Autoridade continua **não revogável** e **não inspecionável** pelo
   usuário: não há chamada para listar os próprios slots.

## H. Próximo passo, na ordem

1. Alocador de quadros + `map` (memória dinâmica), com dono por tarefa e
   falha de esgotamento explícita.
2. `spawn`: criar tarefa a partir de imagem na memória, com as capacidades
   iniciais entregues por `cap_grant` — que é exatamente o que esta fase
   tornou possível.
3. `cap_delete` + `cap_revoke` com cadeia de concessões (a origem zerada
   por `cap_grant` já é o primeiro elo).
4. Carregador/reinício em espaço de usuário (tira 8 621 passos não
   preemptíveis do núcleo).
5. Só então reavaliar o híbrido, com o critério do §B medido em carga
   realista.
