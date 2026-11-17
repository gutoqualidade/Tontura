# experiments/phase5 — segunda implementação da ISA T27

`T27Ref.hs` reexecuta, sobre listas de dígitos (biblioteca Haskell da Fase 1),
cada caso gerado por `src/t27/src/bin/isa_cases.rs` e compara com o emulador.

- Rodar: `tools/isa-crosscheck.sh 30000 [semente]` (ou `tools/verificar-tudo.sh`).
- Resultado guardado: `results/phase5/isa-crosscheck.log` — 3 sementes × 30 000
  casos, 0 divergências, mais o controle negativo.
- **Falso positivo já investigado:** a primeira rodada acusou 4 divergências em
  20 000 casos (589, 8586, 16090, 17717). Eram do GERADOR: quando o endereço de
  dados sorteado coincidia com o PC, a instrução sobrescrevia o dado. O gerador
  agora nunca sorteia `maddr == pc`. Não era defeito da ISA nem do emulador.
