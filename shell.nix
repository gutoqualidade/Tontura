# TENERURA — ambiente reproduzível (NixOS)
# Uso: nix-shell ~/tenerura/shell.nix
# Não instala nada em perfil global; usa somente o store do Nix.
{ pkgs ? import <nixpkgs> {} }:
pkgs.mkShell {
  packages = with pkgs; [
    (pkgs.haskellPackages.ghcWithPackages (hps: [ hps.QuickCheck ]))  # 9.10.x + QuickCheck
    cabal-install
    (python3.withPackages (ps: with ps; [
      numpy
      scipy
      matplotlib
      pandas
      sympy
      statsmodels
      hypothesis
    ]))
  ];
}
