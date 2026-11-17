#!/usr/bin/env bash
# TENERURA — as três verificações da Fase 5 num comando só: testes do
# emulador/núcleo, verificação cruzada Rust × Haskell e TODAS as provas Lean.
# Sai com erro se qualquer uma falhar.
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
out="$("$here/tools/t27-test.sh" 2>&1)" || { echo "$out" | grep -E "FAILED|panicked"; echo "FALHA: testes"; exit 1; }
echo "$out" | grep -E "^test result" | awk '{p+=$4; f+=$6} END {print "testes:", p, "passaram,", f, "falharam"}'
cross="$("$here/tools/isa-crosscheck.sh" 30000 2>&1 | tail -1)"; echo "$cross"
[[ "$cross" == *"divergentes: 0" ]] || { echo "FALHA: verificação cruzada"; exit 1; }
(cd "$here/src/lean" && lake build 2>&1 | tail -1 | grep -q "completed successfully") || { echo "FALHA: provas Lean"; exit 1; }
echo "provas Lean: ok"
