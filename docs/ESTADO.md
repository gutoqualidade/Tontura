# TENERURA — estado corrente (arquivo vivo; ler primeiro ao retomar)

> **Handoff detalhado (log, escolhas, dúvidas, armadilhas):**
> `docs/12-handoff-2026-11-17.md`. Leia-o antes de construir em cima de
> qualquer decisão: o projeto é um protótipo e as escolhas estão lá
> marcadas como hipóteses, com o experimento que falsificaria cada uma.

Atualizado: 2026-11-17 (Fase 10: projeto da linguagem completa — `docs/18`; primeira fatia da etapa 1 implementada).

## Produção, 2026-11-17 (noite) — promoção e correções

`~/tenerura` estava na Fase 5 (idêntica ao commit `d25ac47` do sandbox).
Com autorização do usuário, recebeu a árvore da Fase 10 do sandbox (src,
docs, tools; sem `.git`, `verification/`, `target/`); cópia da Fase 5 em
`~/tenerura-producao-fase5-2026-11-17.tgz`. Linha de base depois da
promoção = a do sandbox (105 testes, 0 divergências, Lean ok). O sandbox
(`~/tenerura-sandbox`, com `verification/AI.md`) fica como evidência.

Corrigido aqui, cada um com teste que falhava antes e passa agora (E2):

| achado | causa | correção | teste |
|---|---|---|---|
| F14: `ir::validate` abortava (SIGABRT, ~38 GB) com contagens corrompidas | limite de nvalores somava `Region.nargs`, campo do próprio arquivo; listas de `extra` podiam se sobrepor | TIR **v3**: `extra`/`strings` canônicos (docs/13 §G nota), limite por bytes reais, `ir::list_len` fonte única, `Module::canonicalize` no `lower` | `tir.rs`: `contagens_corrompidas_…`, `mutacoes_de_varios_bytes_…` (gerador/semente do q4), `listas_e_nomes_ladrilham_…` |
| codificação canônica era falsa (lixo em `extra`/`strings` aceito) | idem | idem | `listas_e_nomes_ladrilham_…` |
| F26: falha antes da checagem de STATUS em `trap_entry` girava para sempre | falha dupla só para PC = TVEC | falha dupla também com MODE = PMODE = núcleo; `eret` faz PMODE←usuário (docs/07 §5 nota) | `kernel.rs::fault_before_the_nested_check_…` |
| F10: `t27 run` com imagem fora da RAM entrava em pânico | `assert!` em `Machine::load` | `load` devolve `Result`, recusa sem escrever | `machine.rs::image_outside_ram_…` |
| novo: `t27r` morria por estouro de pilha com aninhamento fundo | analisador e passes recursivos sem limite | `MAX_NEST`/`MAX_EXPR` com diagnóstico; `t27r` numa thread de 64 MB (docs/18 nota) | `lang.rs::aninhamento_fundo_…` |
| F20/U2: faixa da `word` errada em docs/11 | doc | corrigida com nota | — |
| F4/U3: "628→55" sem origem | doc | 55/51 reproduzidos (semente 213); 628 não regenerável | — |

Conferido também: `.tas` gerado idêntico byte a byte antes/depois nos 25
pares fonte×modo da árvore; varreduras q10b/q10c do sandbox rodadas contra
a produção: os 3 LACO de `trap_entry` viraram `DoubleFault`, nenhum PANICO
mudou. Abertos (não corrigidos, com motivo): R6/F7 limite estático da
pilha do núcleo para T27R `--kernel` (hoje só I9 dinâmico; RAM negativa
livre, estouro termina em pânico); R1/R3 `irgen` ainda decodifica o
`switch` por deslocamento (tamanhos já vêm de `ir::list_len`); R5 sem
interpretador de referência do TIR; R13 `ensure_page` (sem `map` na
produção, latente); ordem de regiões e numeração de valores do TIR ainda
não canônicas.

## Onde estamos

| Camada | Estado | Onde |
|---|---|---|
| Semântica, codificações, ALU (Fases 1–4A) | testado; provas Lean agora de fato verificadas | `src/haskell`, `src/lean`, `src/rust`, `docs/01..05` |
| ISA T27 + emulador | testado; instruções de usuário conferidas contra Haskell | `src/t27/src/{word,isa,machine}.rs`, `docs/07` |
| Montador, construtor de imagem, CLI | testado | `src/t27/src/{asm,sysimage,manifest}.rs`, `bin/t27.rs` |
| Micronúcleo v1 (assembly ternário) | testado (invariantes I1–I12, caos, mutação, cobertura); nada provado | `src/os/kernel/kernel.tas`, `docs/08` |
| Autoridade móvel (`cap_grant`) | implementado e testado; move, não copia; sem revogação | `docs/10` |
| Usuário de demonstração | driver de console, supervisor, serviço instável, cliente, eco | `src/os/user`, `src/os/lib`, `src/os/sys` |
| Linguagem T27R (subconjunto de Rust) | front end próprio → `.tas`; 16 testes; `pingr.t27r` fala com `pong.tas` | `src/t27/src/lang`, `src/t27/src/bin/t27r.rs`, `docs/11` |
| TIR — IR estruturada (2026-11-17, Fase 8) | regiões sem PHI, `word`/`ptr` com proveniência, binário canônico, validação por função (paralela); 2º gerador `--tir`, 2–29% menos instruções de usuário; 10 testes + `lang` roda nos 2 geradores | `src/t27/src/lang/{ir,lower,irgen}.rs`, `docs/13` |
| TIR como alvo do sistema (Fase 9) | T27R só via TIR; ABI, layout e intrínsecas especificados; `irq_enable/irq_disable` do núcleo em T27R; fuzzing diferencial (300 programas) | `docs/14` (relatório + ABI), `15`, `16`, `src/os/kernel/irq.t27r` |
| Avaliação de capacidades da linguagem | 53 itens: existe/planejado/viável/encaixe na ISA/decisão; núcleo mínimo e roteiro em 5 etapas. Só avaliação: nada novo implementado | `docs/17-avaliacao-capacidades.json` |
| Projeto da linguagem completa (Fase 10) | alvo inteiro especificado (tipos `tN`, trit/Kleene, enum de faixas, vistas, lineares, protocolos, `naked fn`, ABI v2, auto-hospedagem com estágio 0 == 1) e 6 etapas. **Implementado:** `bool`/`trit` distintos, `<=>`, `&`/`\|` de Kleene, `match` de trit exaustivo, função sem valor, palavras reservadas, quadro por escopo no backend | `docs/18` (§18 = o que está feito), `src/t27/src/lang` |

Documentos: `06` maturidade e avaliação das recomendações · `07` ISA ·
`08` núcleo · `09` relatório da Fase 5 · `10` Fase 6 (autoridade móvel,
decisão arquitetural e por que o micronúcleo fica) · `11` Fase 7 (linguagem
T27R: por que não `rustc`, o subconjunto, o que falta) · `12` handoff com
log, escolhas questionáveis e o que falsificaria cada uma · `13`–`16` TIR,
ABI, layout, intrínsecas · `17` avaliação de 53 capacidades · `18` projeto
da linguagem completa e etapas · `19` avaliação de ILP, VLIW, DLP e alocação
de registradores (só medição; recomenda ordem de trabalho).

## Como rodar

```
tools/verificar-tudo.sh           # tudo: 111 testes + emulador × Haskell + provas Lean
tools/t27-test.sh                 # só os testes (nix-shell + cargo test)
T27_DEPTH=5 tools/t27-test.sh --test exhaustive -- --nocapture   # exaustão mais funda (~35 s)
cd src/t27 && nix-shell ../../shell-rust.nix --run \
  'cargo run --release --bin t27 -- sys ../os/sys/demo2.sys 20000000 --input "ola ternura|q"'
```

## Armadilhas já pisadas

- `lake build` "com sucesso" não prova nada se o módulo não está no alvo:
  conferir "Built TeneruraProofs.<Módulo>" ou `#print axioms`.
- Programa de teste com `.data` no fim: a biblioteca incluída depois cai
  na seção de dados (os ajudantes `prog`/`prog_drv` já voltam a `.text`).
- Mudou o fonte do núcleo? Os mutantes de `tests/mutants.rs` casam por
  texto e acusam trecho não encontrado.

## Próximos passos (docs/10 §H)

1. Alocador de quadros + `map` (memória dinâmica), com dono por tarefa e
   falha de esgotamento explícita.
2. `spawn`: criar tarefa a partir de imagem na memória, entregando as
   capacidades iniciais por `cap_grant` (o que a Fase 6 tornou possível).
3. `cap_delete` + `cap_revoke` com cadeia de concessões.
4. Carregador/reinício em espaço de usuário (tira 8 621 passos não
   preemptíveis do núcleo).
5. Exaustão mais funda (poda por estado abstrato); depois modelo abstrato +
   prova do núcleo.
6. Cache de tradução no emulador; segunda implementação de traps/tradução.
7. Linguagem T27R: resto da etapa 1 do `docs/18` (§18.4: múltiplos
   resultados/ABI v2, `loop`/`break`, `never`, interpretador de referência
   do TIR); depois a etapa 2 (dados). Substitui a ordem do `docs/11` §G.
8. Só então reavaliar micronúcleo × híbrido, com o critério medido do
   docs/10 §B.
