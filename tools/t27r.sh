#!/usr/bin/env bash
# TENERURA — compila um fonte T27R (.t27r) para assembly T27 (.tas).
# Uso: tools/t27r.sh programa.t27r [-o saida.tas]
# O .tas gerado inclui ../lib/abi.inc e ../lib/ulib.inc: deixe-o em
# src/os/user/ (ou num diretório com um ../lib ao lado).
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
exec nix-shell "$here/shell-rust.nix" --run \
  "cargo run --quiet --manifest-path '$here/src/t27/Cargo.toml' --bin t27r -- $*" \
  2> >(grep -v 'Nix search path entry' >&2)
