#!/usr/bin/env bash
# TENERURA — verificação cruzada da ISA T27: emulador Rust ↔ referência
# Haskell independente (experiments/phase5/T27Ref.hs).
# Uso: tools/isa-crosscheck.sh [n_casos] [semente]
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
n="${1:-20000}"
seed="${2:-}"
cases="$here/src/t27/target/isa-cases.txt"
nix-shell "$here/shell-rust.nix" --run "cd '$here/src/t27' && cargo build --release --bin isa_cases 2>/dev/null && ./target/release/isa_cases $n $seed > '$cases'" 2> >(grep -v 'Nix search path' >&2)
echo "casos gerados: $(wc -l < "$cases") (semente ${seed:-padrão})"
nix-shell "$here/shell.nix" --run "cd '$here/src/haskell' && ghc -O1 -isrc -outputdir '$here/src/t27/target/hs' -o '$here/src/t27/target/t27ref' '$here/experiments/phase5/T27Ref.hs' >/dev/null && '$here/src/t27/target/t27ref' < '$cases'" 2> >(grep -v 'Nix search path' >&2)
