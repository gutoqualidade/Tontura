# Fase 3D — Síntese, matriz de veredicto e decisão do gate

## Matriz de cruzamento de evidências

| Propriedade | A (simetria) | B (redundância) | C (hierarquia) | Reforço/conflito |
|---|---|---|---|---|
| Cardinalidade ímpar ⇒ centro único + pares opostos (T2, exaustivo) | ✔ matemática | pré-condição da votação por maioria | pré-condição da faixa balanceada por nível | **reforça** as três |
| Ímpar ⇒ empates impossíveis na votação sem apagamentos (P1/P2) | consequência do centro | vantagem concreta do r ímpar sobre r par | — | **reforça A×B** |
| Ímpar ⇒ faixa balanceada simétrica ∀n (T5) | ✔ | não usa | não exige n ímpar de trits | independente |
| Aninhamento sem resíduo | não usa | não usa | EXCLUSIVO de 3ⁱ (C1) | independente de A e B |
| Empacotamento binário ótimo (n=5,10,20,40,80) | não usa | não usa | CONFLITA com 3ⁱ como contagem de estados | **conflito interno em C** |
| Guardas multiníveis convertem erro em apagamento | — | mecanismo de detecção (P3) | — | **reforça B** (independente de paridade) |
| Ruído correlacionado degrada maioria | — | limita B (burst sempre pior) | — | condição de contorno |

Propriedades que se reforçam: ímpar-centro (T2) alimenta simultaneamente a
votação (B) e a faixa por nível (C). Propriedades independentes: T5 (simetria
da faixa ∀n) e o aninhamento 3ⁱ (C). Conflito: contagem de estados (quer 3ⁱ)
vs contagem de trits por contêiner (quer convergentes 5, 10, 20, 40, 80).

## Arquiteturas candidatas (modelos conceituais)

- **A — simetria balanceada**: palavra de n trits (qualquer n), faixa
  [−(3ⁿ−1)/2, (3ⁿ−1)/2], negação digitwise, truncamento = arredondamento (A6).
  Suporte: T1–T5 + Fase 2. Custo: nenhum demonstrado. **Viacável e adotada
  como semântica da ALU** (sem exigir n ímpar de trits).
- **B — redundância ímpar**: repetição r ímpar + maioria para canais
  não-confiáveis; apagamentos quando disponíveis. Suporte: P1/P2. **Adotada
  como MECANISMO OPCIONAL de camada de transporte**, não como semântica da
  ALU (a ALU não vota). Fica experimental quanto a custo/energia.
- **C — hierarquia heterogênica**: níveis de estados em 3ⁱ (aninhamento sem
  resíduo), larguras de contêiner em convergentes (n=5 para byte; n=40 para
  64 bits). Suporte: C1–C3. **Decisão de ISA NÃO antecipada** — o custo de
  endereçamento em radix mista não foi medido.
- **D — combinação**: NÃO confirmada como princípio único. O que existe é um
  núcleo comum honesto: *cardinalidade ímpar ⇒ centro único* — que é
  matematicamente trivial (2x≡0 mod N ímpar ⇒ x≡0) e cujo valor arquitetural
  vem dos usos concretos (votação, faixa), não de uma "força" dos ímpares.

## Matriz de veredicto

| Hipótese | Matemática | Experimento | Consequência arquitetural | Veredicto |
|---|---|---|---|---|
| A — Simetria | Confirmada (T1–T5 Lean + exaustivo) | Confirmada (exp3a, N=1..64, n=1..12) | **Parcial**: negação digitwise e truncamento=arredondamento usados na ALU; centro único não cria op adicional | **Confirmada com valor prático limitado** |
| B — Estabilidade | Parcial: t=⌊(r−1)/2⌋ e 2e+s≤r−1 (código, não paridade) | Confirmada (MC 4 modelos, IC95); burst degrada; apagamento ≫ substituição | Parcial: mecanismo opcional de transporte; sem justificativa de custo físico | **Parcialmente confirmada** |
| C — Hierarquia | Confirmada (estrutural: 3ⁱ exclusivo p/ aninhamento) | Confirmada (C1–C3, determinístico) | Conflito estados×contêiner ⇒ hierarquia heterogênea; custo de endereçamento NÃO medido | **Parcialmente confirmada, decisão de ISA pendente** |
| D — Combinação | NÃO demonstrada como princípio único | núcleo comum identificado (centro ímpar), mas sem força unificadora adicional | não formulável sem medir custo de C | **Inconclusiva/Refutada como "princípio único"; propriedades independentes coexistem** |

## Decisão do gate (§6)

1. Semântica aritmética bem definida: **SIM** (ternário balanceado, Fase 1,
   A6 truncamento = arredondamento sem empates).
2. Representação consistente: **SIM** (pack2/pack3 canônicas, Fase 2,
   extensão de sinal = replicação do trit de topo).
   > **CORREÇÃO (2026-11-16, Fase 4A):** a afirmação entre parênteses está
   > REFUTADA pelos testes da Fase 4 (e confirmada pela referência Haskell,
   > que sempre preencheu com Z): no ternário balanceado todos os pesos são
   > positivos, e replicar o trit de topo MUDA o valor (ex.: [−1,−1] = −4 →
   > [−1,−1,−1] = −13). O contrato correto é **extensão de largura com
   > preenchimento ZERO**, preservando valor e canonicidade — ver
   > docs/05-alu-especificacao.md e o doc de `width_extend` em alu.rs.
3. Regras de overflow/truncamento explícitas: **SIM** — overflow de soma =
   carry final ≠ 0 (resultado = mod balanceado 3ⁿ); shl overflow ⟺ trit de
   topo ≠ 0 (demonstrável: |v|≥(3ⁿ⁻¹+1)/2 > (3ⁿ−1)/6 ⟺ |3v|>M); mul exato em
   i128 e truncado; shr = divisão por 3 com arredondamento (queda do LSB
   exata, sem perda).
   > **CORREÇÃO (2026-11-16, Fase 4A):** "sem perda" é INCORRETO. shr = queda
   > do LSB = arredondamento de V/3 ao inteiro mais próximo (sem empates);
   > o LSB descartado É o resto — HÁ perda de informação (shl∘shr zera o
   > LSB: V → V − d₀). shl∘shr ≠ V em geral; shr∘shl = V ⟺ shl não estourou.
4. Operações especificáveis: **SIM** (add/sub/neg/mul/cmp/shl/shr).
5. Contradições críticas: **NENHUMA** encontrada (3A–3C não contradizem a base).
6. Separação emulação/arquitetura: **SIM** (ALU sobre dígitos Vec<i8>,
   independente de pack2/pack3; hipóteses não resolvidas — custo físico de B,
   custo de endereçamento de C — permanecem FORA da semântica da ALU).

**GATE: PASSA. Fase 4 autorizada.** As hipóteses em aberto ficam isoladas: a
ALU usa apenas A (semântica balanceada) + regras de overflow acima; B é camada
opcional; C não determina ISA nesta fase.
