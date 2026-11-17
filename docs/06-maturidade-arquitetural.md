# Fase 5 — Investigação de maturidade arquitetural do núcleo TENERURA

Data: 2026-11-16. Pergunta: o que a TENERURA precisa para ser um sistema
operacional robusto, o que disso existe, e que decisões ainda não têm
evidência. Níveis de evidência usados: **especificado → implementado →
executado → testado → verificado por implementação independente → provado
(Lean)**; e **proposto**, **aberto**.

## 1. Estado real encontrado (antes desta fase)

| item | alegado | encontrado |
|---|---|---|
| Semântica, codificações, ALU (Fases 1–4A) | implementado e testado | **confirmado**: 47 testes Rust verdes, reproduzidos |
| Provas Lean das Fases 3A e 4A (22 teoremas nas tabelas dos relatórios; 30 declarações nos arquivos) | "provado, build limpo" | **NÃO verificado**: `Phase3.lean` (20 erros) e `Phase4A.lean` (34 erros) não compilavam; o `lake build` só construía o executável, que importa apenas `Basic.lean`. Só `bval_shift` (Fase 1) estava de fato provado. Corrigido nesta fase (docs/09, achado L1) |
| ISA | "próxima etapa" | inexistente |
| Plataforma de execução | — | inexistente (nenhum emulador, montador, formato de imagem) |
| Núcleo | "código de núcleo existente" (premissa da tarefa) | **inexistente**: não havia uma linha de núcleo no repositório |

Conclusão: o projeto tinha a camada matemática e uma ALU de referência.
Entre isso e um sistema operacional faltavam TODAS as camadas
intermediárias.

## 2. Maturidade por camada (depois desta fase)

| camada | nível | evidência | o que falta para subir |
|---|---|---|---|
| Semântica e ALU | provado (agora de verdade) + testado | `lake build` constrói os 4 módulos; 46 teoremas, sem `sorry` | laço de `from_i64` continua só testado |
| ISA T27 (instruções de usuário) | verificado por implementação independente | emulador × Haskell, 90 000 casos | modelo formal da ISA |
| ISA T27 (privilégio, traps, tradução) | testado, UMA implementação | 12 testes de máquina + todos os de sistema | segunda implementação; TLB real no emulador |
| Montador | testado | 7 testes + todo o sistema montado por ele | — |
| Micronúcleo v1 | testado (nada provado) | invariantes a cada saída do núcleo, caos, mutação (50/50), cobertura 99,81% | modelo abstrato + verificação de modelo; prova |
| Serviços de usuário | demonstração | driver de console, supervisor, serviço instável, cliente | carregador, memória dinâmica, nomes, armazenamento |
| Microarquitetura, hardware físico | nada | — | fora do escopo; nenhuma afirmação |

## 3. Avaliação das recomendações (arquivo `Anti-patterns`)

A lista mistura três alvos sem dizer qual: (H) o programa Rust do
hospedeiro (emulador, montador), (G) o software ternário convidado
(núcleo, tarefas), (F) um hardware ternário físico que não existe. Aplicar
uma regra de H a G ou F é o erro conceitual central.

| categoria | onde vale | veredito |
|---|---|---|
| memória (heap, Vec, clones, padding) | H | válida para o caminho quente do emulador; **incorreta** se usada contra a ALU `Vec<i8>`, que é oráculo e não caminho quente. Em G, "preferir memória estática" foi adotada (núcleo sem heap). Padding/alinhamento **não existem** em G: tudo é palavra |
| CPU (divisões, desvios, SIMD, inlining) | H | válida só com medição; medido: a decodificação custa mais que a aritmética. Para G/F não há modelo de custo: inaplicável |
| Rust | H | válida; o emulador não tem `unsafe`, dependências nem despacho dinâmico |
| cache, TLB, NUMA, false sharing | H, e F hipotético | **inaplicável** à arquitetura: T27 não tem modelo de cache. Usar isso para escolher larguras de palavra (5/20/40 trits "porque cabem em u8/u32/u64") deixaria o hospedeiro ditar a arquitetura — rejeitado |
| hardware/firmware (MMIO, volatile, endianness, DMA) | parcialmente G | "MMIO não é RAM": válida e adotada (dispositivos por página, só no driver). Endianness **dissolve-se** numa máquina endereçada por palavra. `volatile`, barreiras, ordenação de memória: inaplicáveis hoje — mas apontam um requisito ausente: definir o modelo de memória antes de qualquer SMP |
| arquitetura de núcleo | G | "minimizar dependências, tratadores curtos, escalonador simples, memória estática": adotadas. "Evitar trocas de contexto" **conflita** com a direção de micronúcleo e precisa ser lida como "tornar o IPC barato". "Evitar travas globais / estruturas por CPU": prematuras — um núcleo não preemptível de um processador não tem trava nenhuma |
| ISA e assembly | G/F | válidas e adotadas (decodificação fixa e estrita, um modo de endereçamento, sem efeitos colaterais ocultos). "Área de decodificador, latência de pipeline": inavaliáveis sem modelo de hardware |
| código gerado por IA | H e G | válida; a mais relevante provou-se "afirmar sem medir/verificar" — ver achado L1 |
| técnicas avançadas (RCU, lock-free, PGO, pointer tagging…) | H | **desnecessárias** agora; nenhuma tem gargalo medido que a justifique |
| ferramentas (QEMU, Renode, Criterion, Miri, fuzz) | H | QEMU/Renode **não servem** (não há alvo ternário): foi preciso um emulador próprio. Criterion/cargo-fuzz violariam a política de zero dependências; substituídos por arnês próprio com sementes fixas |
| regras de engenharia | H e G | válidas; "correção antes de desempenho", "consumo limitado de recursos", "pior caso" foram tratadas como requisitos do núcleo |

**O que falta na lista** (e é pré-condição de tudo o que ela recomenda):
uma plataforma de execução definida; uma especificação com segunda
implementação; cadeia de ferramentas; determinismo e reprodutibilidade;
modelo de autoridade (capacidades, nenhuma autoridade ambiente); modelo de
falhas (o que é contido, quem é avisado, quem recupera); isolamento
TEMPORAL além do espacial; semântica de tempo (volta do relógio); protocolo
de boot a partir de memória não zerada; observabilidade; e verificação da
própria verificação (testes de mutação, build que realmente cobre as provas).

## 4. O que precisa existir (requisito × mecanismo × estado)

| requisito fundamental | mecanismo escolhido (decisão atual) | estado |
|---|---|---|
| plataforma com semântica definida | ISA T27 + emulador determinístico | testado; usuário verificado em 2 implementações |
| separação de privilégio | 2 modos; opcode negativo = privilegiado | testado |
| isolamento espacial | paginação base 3, núcleo fora dos espaços de usuário, W^X | testado (I8 + sonda) |
| isolamento temporal | fatia de tempo; núcleo de trechos limitados | testado; limite medido (527 / 8621 passos) |
| comunicação controlada | IPC síncrono por registradores + capacidades por slot | testado |
| interface total do núcleo | toda entrada tem resultado definido; nenhum ponteiro de usuário | testado (caos, 354 mil chamadas) |
| contenção de falha | morte + erro aos dependentes + prazo em toda espera | testado |
| recuperação | aviso de morte + reinício sob capacidade; política no supervisor | testado |
| recursos limitados | tudo estático | por construção |
| dispositivos fora do núcleo | página de dispositivo + interrupção como mensagem | testado (console) |
| boot de estado indefinido | tabela de boot validada; nada depende de memória zerada | testado |
| garantia | invariantes em execução, caos, mutação, cobertura, 2ª implementação, Lean | parcial: nada do núcleo é provado |
| sistema dinâmico (criar tarefas, memória, revogar capacidades) | — | **proposto**; transferir capacidades já existe (`cap_grant`, docs/10) |
| carregador, nomes, armazenamento, sistema de arquivos | — | **aberto** |
| tolerância a falha de hardware (trit corrompido, memória) | — | **aberto** (liga-se à Fase 3B) |

Mecanismos considerados **desnecessários** e deixados de fora: registrador
de flags; três famílias de deslocamento; extensão de sinal; terceiro modo
de privilégio; núcleo mapeado no espaço do usuário; pilha de núcleo por
tarefa; heap de núcleo; filas encadeadas; objeto "endpoint" no núcleo;
drivers no núcleo; ponto flutuante; travas.

## 5. Dependências entre subsistemas (as que não eram óbvias)

1. Núcleo em software ternário ⇒ exige montador ⇒ exige ISA com privilégio
   e traps. Não há atalho pela ALU.
2. Contenção de falha depende de PREEMPÇÃO (timer) tanto quanto de
   proteção de memória: sem ela um laço infinito compromete todos.
3. Preempção depende de semântica de tempo sã: a primeira versão permitia
   a uma tarefa estragar a base de tempo com um `sleep` longo.
4. Driver em espaço de usuário depende de RESPOSTA ADIADA no IPC (guardar
   um leitor enquanto espera a interrupção) — isso obrigou a trocar o
   "responde ao último chamador" por recibos explícitos.
5. Clientes robustos dependem de PRAZO em toda espera, porque com resposta
   adiada o núcleo não distingue servidor lento de servidor travado.
6. Supervisão depende de aviso de morte + capacidade de tarefa + imagem
   original preservada; e a origem do aviso precisa ser infalsificável
   (selos negativos reservados ao núcleo).
7. Nenhuma afirmação de isolamento vale sem autoridade explícita: o
   console "de todos" foi um canal de interferência achado pelo teste de caos.
8. Afirmações formais dependem de o build realmente compilar as provas.

## 6. Alternativas consideradas

| decisão | alternativas | por que a atual |
|---|---|---|
| núcleo = software ternário na máquina emulada | núcleo em Rust no hospedeiro gerenciando tarefas ternárias | o segundo não demonstraria que a ISA sustenta um núcleo, e o isolamento seria do programa hospedeiro |
| micronúcleo (direção herdada, mantida) | monolítico; núcleo de separação estático; exonúcleo; espaço único com proteção por linguagem | sem compilador, tudo que é privilegiado é escrito à mão: menor é verificável (1028 palavras, 99,81% cobertas). O v1 é, de fato, quase um núcleo de separação com supervisão; nenhuma evidência pede mudança de direção |
| paginação | segmentos base/limite; máquina de capacidades com memória etiquetada | paginação em base 3 aplica direto os resultados 3ⁱ e A6; capacidades em hardware ficam como questão aberta, não descartadas |
| IPC síncrono em registradores | filas assíncronas no núcleo; memória compartilhada | sem buffer no núcleo (nada para esgotar), sem ponteiro de usuário. Mensagens grandes precisarão de memória compartilhada: **proposto** |
| núcleo em assembly | linguagem de sistemas + compilador para T27 | menor caminho até um sistema que roda. O compilador existe desde a Fase 7: **T27R**, subconjunto de Rust com front end próprio (`docs/11`); o núcleo continua em assembly, por escolha |
| palavra em `i64` no emulador | 2 planos de bits; vetor de dígitos | medido: 0,9 ns contra 23,6 e 42,4 ns por instrução na mistura real |

## 7. Decisões sem evidência suficiente

1. Palavra de 27 trits (não comparada com 18/24/81).
2. Endereçamento por palavra (densidade de código e texto não medida).
3. Página de 729 palavras; espaço virtual de 18 trits; 2 níveis.
4. 27 registradores; instrução de largura fixa.
5. Divisão centrada como semântica de `div` (coerente e provada, mas sem
   experiência de uso em software real).
6. IPC síncrono; varreduras O(n); ausência de prioridades — não escalam
   além de poucas tarefas.
7. Reinício dentro do núcleo.
8. Dois modos de privilégio bastam (não se achou uso para um terceiro; não
   se provou que não existe).
9. Qualquer vantagem do ternário em custo, energia ou desempenho:
   **nenhuma evidência**, nem a favor nem contra.

## 8. Demonstrações

| demonstração exigida | estado |
|---|---|
| inicializa a partir do reset, inclusive com RAM cheia de lixo | feita (testada) |
| executa cargas independentes em espaços isolados | feita |
| duas tarefas se comunicam | feita |
| falha de uma não compromete a outra nem o núcleo (14 classes de falha; comportamento arbitrário) | feita (testada por amostragem — não é prova) |
| tarefa que não cede não trava o sistema | feita |
| serviço que falha é reiniciado e o cliente se recupera | feita |
| driver fora do núcleo com interrupções | feita (console) |
| o próprio núcleo falha de modo seguro | feita (pânico) |
| não-interferência provada | **pendente** |
| sistema dinâmico (criar/destruir tarefas em execução) | **pendente** |
| carga útil real (algo além de demonstrações) | **pendente** |

## 9. Questões em aberto

Modelo de memória concorrente para SMP; capacidades em hardware (palavra
etiquetada: dado / capacidade / inválido) como alternativa à paginação;
detecção de trit inválido como trap de máquina (herança do código `11` do
pack2); linguagem de sistemas para T27; formato de texto denso (3 trytes de
9 trits por palavra); medição do custo de endereçamento num modelo de
portas de verdade.
