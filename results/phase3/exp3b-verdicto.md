# Fase 3B — Veredicto (resumo técnico)

Experimento: `experiments/phase3/exp3b_redundancy.py` · Saídas: `exp3b_p1.csv`,
`exp3b_p2.csv`, `exp3b_p3.csv` (brutos). Semente base `0x5EED3B01`, derivações
de semente por crc32 (reprodutível entre processos; `hash()` de str foi
descartado por ser randomizado). n = 100 000 palavras/config (P2), exato (P1, P3).

## P1 — Código de repetição (exaustivo por composições)

Completude do método: para palavra canônica [s]ʳ, a maioria depende só da
composição do multiset de votos (corretos r−e−s, errados e divididos entre os
dois símbolos opostos, apagados s) — posições são irrelevantes. TODAS as
composições foram verificadas.

- Correção de substituições até t = ⌊(r−1)/2⌋: **confirmada para todo r testado**
  (1..9, 11, 13), todos os 3 símbolos (sem apelo à simetria).
- Primeira falha sempre ocorre com e ≥ 1 em 2e+s = r (empate ou derrota da
  maioria) — coerente com o limite t.
- Orçamento combinado 2e+s ≤ r−1 ⇒ decodificação correta: **confirmado**
  (propriedade do código de repetição, não do r ímpar em si — vale também para
  r par com a mesma regra, exceto empates).
- r par com regra empate→apagamento: nunca decodifica símbolo errado em empate
  (apagamento), mas t = 0 e falha já com 1 erro (r=2,4,6): **menor capacidade
  por componente gasto que o ímpar imediato** (r=2 falha em (1,0); r=3 corrige 1).

## P2 — Monte Carlo (4 modelos × 10 r × 5 p; IC de Wilson 95%)

Leituras (p=0,01 → p=0,1, taxa residual de falha):
- indep: r=1 0,68%→6,6%; r=3 0,013%→1,3%; r=5 0%→0,26%; r=7 0%→0,033%; r=9 ~0→0,001%.
- burst (rajadas de 3 + fundo): sempre PIOR que indep no mesmo r (r=3 p=0,1:
  2,2% vs 1,3%) — correlação local degrada a independência que a maioria assume;
  a degradação relativa diminui com r (r=9: 0,016% vs 0,001%).
- asym (→0 com p/2, →±1 com p/4 cada): praticamente igual ao indep — a maioria
  é insensível à assimetria entre destinos (contagem é simétrica nos símbolos).
- erasure (só apagamentos, mesma p): MUITO melhor — r=3 p=0,1: 0,087% vs 1,3%
  do indep (~15×). Apagamento é um erro benigno: 2e+s ≤ r−1 gasta s com peso 1.
- Custo honesto: r=5 gasta 5 componentes para transportar 1 trit lógico
  (fator 5); a comparação "mesma quantidade de informação" exige comparar
  taxas POR COMPONENTE: r=1 p=0,1 → 6,6%/comp; r=3 → 1,3%/3 = 0,42%/comp;
  r=5 → 0,26%/5 = 0,052%/comp. Ganho por componente real, decrescente.

## P3 — Níveis multiníveis (exato, gaussiano)

L ∈ {3,5,7,9} níveis físicos, lógicos {−h,0,h}, guardas intermediárias → apagamento.
- P_ok é IDÊNTICO para todo L com o mesmo σ (2Φ(0,5/σ)−1): guardas NÃO aumentam
  a probabilidade de acerto — o ruído é o mesmo.
- Guardas convertem substituição em apagamento: σ=0,5 → L=3: 21,2% substituições
  não detectadas; L=5: 0,18% sub + 21,0% apag; L=7: 0% sub + 21,2% apag.
- σ=0,75: L=9 → 0% sub; σ=1,0: L=9 → 0,03% sub; σ=1,5: L=9 → 1,2% sub.
- OU SEJA: mais níveis físicos compram DETECÇÃO (substituição→apagamento), não
  correção direta; correção exige redundância (P1/P2) ou decodificação iterativa.

## Veredicto B

- **Robustez matemática**: t = ⌊(r−1)/2⌋ e 2e+s ≤ r−1 são propriedades do
  código de repetição com maioria — valem para qualquer r; a VANTAGEM do r
  ímpar sobre o par é concreta mas limitada: empates impossíveis sem apagamentos
  e t maior para o mesmo r (r par tem t = r/2 − 1 < ⌊(r−1)/2⌋... na prática
  r=2k falha com k erros onde r=2k+1 corrige k).
- **Robustez estatística simulada**: confirmada (tabelas acima), com a ressalva
  de que a comparação justa é por componente transportado.
- **Ruído correlacionado** degrada o ganho — hipótese "ímpar ⇒ robusto"
  NÃO se sustenta sozinha sob correlação (burst sempre pior).
- **Apagamento >> substituição**: converter erro em apagamento (guardas,
  códigos com distância maior) vale mais que a paridade de r.
- **Robustez física**: NÃO demonstrada (amplitude abstrata, sem dispositivo).
- Hipótese genérica "quantidades ímpares de níveis/components ⇒ mais estáveis":
  a parte real é "cardinalidade ímpar elimina empates na votação e dá t=(r−1)/2";
  o resto é propriedade de códigos, não de ímpares.

## Limitações

- Simulação estatística, não circuito; modelo de amplitude gaussiano abstrato.
- Correlação modelada só como rajadas locais de 3 componentes.
- P2 usa palavra canônica única (justificado pela simetria de símbolos de P1);
  código de repetição só transporta 1 trit lógico por palavra de r.
- P3 não modela quantização de dispositivo, deriva de limiar, nem ruído
  correlacionado entre níveis.
