# Fase 3C — Veredicto (resumo técnico)

Experimento: `experiments/phase3/exp3c_hierarchy.py` · Saídas: `exp3c_c1_familias.csv`,
`exp3c_c2_arvores.csv`, `exp3c_c3_packing.csv`. Determinístico (sem amostragem).

**Distinção metodológica essencial** (registrada para não contaminar a 3D):
- contagem de ESTADOS (3ⁿ estados de uma palavra de n trits) — hierarquia de
  memória/agrupamentos vive aqui;
- contagem de TRITS por unidade (n trits empacotados) — registradores e
  contêineres binários vivem aqui.
Um grupo de 5 ESTADOS não é um número inteiro de trits (log₃5 ≈ 1,465) — ímpar
≠ decomponível em trits.

## C1 — Famílias

- Só as potências de 3 têm resíduo ZERO em todos os níveis ternários
  simultaneamente (n%3 = n%9 = n%27 = 0): aninhamento ternário sem sobras.
- Compostos ímpares com fator 3 (15, 21, 33, 45, 63) aninham UM nível ternário
  e deixam um subgrupo não-ternário (5, 7, 11) — hierarquia mista possível.
- Primos > 3 (5, 7, 11, 13, 17...) são átomos: nenhum aninhamento em grupos;
  úteis apenas como contagens atômicas (larguras, lanes), não como níveis.
- Mersenne 2^m−1: 3 é potência de 3; 7, 31, 127, 8191 são primos (átomos);
  511=7·73 e 2047=23·89 aninham, mas não em base 3 (n%3=1). Vantagem binária
  (máscaras all-1) é propriedade BINÁRIA, sem papel ternário demonstrado.

## C2 — Árvores de composição (mesmo total de estados)

Para T = 729: 11 árvores válidas, todas com fatores ímpares ⇒ faixa balanceada
preservada em TODOS os níveis (consequência direta do T5 da Fase 3A — simetria
não exige nada além de cardinalidade ímpar por nível). Só a árvore 3⁶ é
uniforme E puro base-3 (1 radiz); 9·9·9 e 27·27 são uniformes com 1 radiz
não-ternária; 3·9·27 usa 3 radizes distintas (endereçamento de radix mista).
Nenhuma árvore tem resíduo (fatoração exata por construção); resíduos surgem
quando o total não é divisível pelo tamanho do grupo (C1: res%3, res%5, res%7).

## C3 — Empacotamento binário de n trits (bits = ⌈n·log₂3⌉)

Melhor n por largura de contêiner (máximo que cabe):

| Largura | n trits | bits usados | desperdício |
|---:|---:|---:|---:|
| 8 | 5 | 8 | 0,075 |
| 16 | 10 | 16 | 0,150 |
| 32 | 20 | 32 | 0,301 |
| 64 | 40 | 64 | 0,602 |
| 128 | 80 | 127 | 0,203 |

Convergentes de log₂ 3 (pontos de razão ideal): n = 2, 5, 12, 41, 53, 306 com
erro 0,085 → 0,015 → 0,0016 → 0,0004 → 6e-5 → 3e-6.

Leitura: a eficiência de empacotamento NÃO privilegia exclusivamente n ∈ {3ⁱ}
(3 trits → 5 bits, desperdício 0,339; 9 trits → 15 bits, desperdício 0,735);
n = 5, 10, 20, 40, 80 são melhores contêineres binários. Os interesses de
aninhamento ternário (estados = potência de 3) e de empacotamento binário
(trits por contêiner = convergentes) APONTAM PARA DIREÇÕES DIFERENTES.

## Veredicto C

- **Propriedade exclusiva das potências de 3**: aninhamento ternário sem
  resíduo em profundidade arbitrária (única família com n%3ᵏ=0 ∀k) e razão
  estados/níveis constante. Confirmada (estrutural).
- **Propriedade compartilhada por TODOS os ímpares**: faixa balanceada e ponto
  central existem em qualquer nível ímpar (T5/T1 da 3A) — não exclusiva de 3ⁱ.
- **Sem consequência arquitetural demonstrada**: primalidade dos Mersennes
  grandes; nº de divisores como métrica isolada.
- **Conflito real**: contagem de estados quer 3ⁱ; contagem de trits por
  contêiner binário quer convergentes (5, 12, 41...). Hierarquia HETEROGÊNEA:
  níveis de estados em 3ⁱ, larguras de contêiner/registrador em n otimizado
  por largura (n=5 p/ byte é o caso mais expressivo: 0,94% de desperdício).
- **Questão aberta**: custo de endereçamento em radix mista vs puro base-3
  (métrica combinatória aqui; benchmark real pendente — não inventado).

## Limitações

Métricas estruturais/combinatórias apenas; nenhum benchmark de endereçamento;
custo de decodificação de radix mista não medido; larguras >128 bits não
exploradas (convergente n=306 inatingível em contêiner prático).
