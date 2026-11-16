#!/usr/bin/env bash
# TENERURA — roda a suíte da máquina T27 (emulador, montador, núcleo).
# Uso: tools/t27-test.sh [argumentos extras do cargo test]
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
exec nix-shell "$here/shell-rust.nix" --run "cd '$here/src/t27' && cargo test $*" 2> >(grep -v 'Nix search path entry' >&2)
