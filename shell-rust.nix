# TENERURA — ambiente Rust (NixOS): linker C do Nix + toolchain rustup existente.
# Uso: nix-shell ~/tenerura/shell-rust.nix
{ pkgs ? import <nixpkgs> {} }:
pkgs.mkShell {
  packages = with pkgs; [
    gcc              # cc/linker — o toolchain rustup não traz linker em NixOS
    pkg-config
  ];
  shellHook = ''
    export PATH="$HOME/.rustup/toolchains/1.99.0-x86_64-unknown-linux-gnu/bin:$PATH"
  '';
}
