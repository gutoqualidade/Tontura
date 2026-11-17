# TENERURA

Micronúcleo e máquina ternária **T27** (palavra de 27 trits balanceados): emulador,
montador, núcleo em assembly `.tas` e compilador da linguagem T27R.

## Boot

```bash
nix-shell shell-rust.nix --run "cd src/t27 && cargo run --release --bin t27 -- sys ../os/sys/demo1.sys"
```

`t27 sys <manifesto>` monta o núcleo e as tarefas do manifesto e executa. Também:
`t27 asm <arq.tas>` (listagem) e `t27 run <arq.tas>` (máquina nua).
Testes: `tools/t27-test.sh`. T27R → `.tas`: `tools/t27r.sh programa.t27r`.

## Estrutura

- `src/t27/` — emulador, montador, compilador T27R/TIR e testes (Rust, sem dependências)
- `src/os/kernel/` — micronúcleo (`kernel.tas`, `irq.t27r`)
- `src/os/lib/` — ABI e bibliotecas de usuário
- `src/os/user/` — programas de usuário; `src/os/sys/` — manifestos de sistema

As datas dos commits das primeiras fases são aproximadas (reconstruídas dos horários
de modificação dos arquivos; o projeto não usava git no início).
