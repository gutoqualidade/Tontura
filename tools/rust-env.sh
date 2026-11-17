#!/usr/bin/env bash
# TENERURA — acesso ao toolchain Rust já existente (sem reinstalar nada).
# Os shims do rustup não estão no PATH; esta variável aponta direto para a
# toolchain estável presente em ~/.rustup (1.99.0, registrada no diagnóstico §8.1).
export RUST_TOOLCHAIN="$HOME/.rustup/toolchains/1.99.0-x86_64-unknown-linux-gnu/bin"
export PATH="$RUST_TOOLCHAIN:$PATH"
