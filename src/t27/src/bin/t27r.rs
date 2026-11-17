//! `t27r` — compilador do subconjunto de Rust para T27.
//!
//! Uso:
//!   t27r programa.t27r            → escreve programa.tas
//!   t27r programa.t27r -o saida.tas
//!   t27r -                        → lê a entrada padrão, escreve na saída
//!
//!   t27r programa.t27r --pilha    → .tas pelo gerador de pilha antigo (oráculo)
//!   t27r modulo.t27r --kernel     → biblioteca .tas do núcleo (docs/14)
//!   t27r programa.t27r --emit-ir  → texto do TIR (só leitura humana)
//!   t27r programa.t27r --emit-tir → binário .tir (o formato canônico)
//!
//! O `.tas` gerado inclui `../lib/abi.inc` e `../lib/ulib.inc`: coloque-o
//! em `src/os/user/` (ou em qualquer diretório com `../lib`).

use std::process::ExitCode;

use tenerura_t27::lang;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut input: Option<String> = None;
    let mut output: Option<String> = None;
    let mut mode = "tas";
    let mut kmode = lang::Mode::User;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-o" => {
                i += 1;
                match args.get(i) {
                    Some(p) => output = Some(p.clone()),
                    None => {
                        eprintln!("t27r: -o precisa de um caminho");
                        return ExitCode::from(2);
                    }
                }
            }
            "--pilha" | "--emit-ir" | "--emit-tir" => mode = &args[i][2..],
            "--kernel" => kmode = lang::Mode::Priv,
            "-h" | "--help" => {
                println!("uso: t27r <programa.t27r | -> [-o saida] [--kernel] [--pilha | --emit-ir | --emit-tir]");
                return ExitCode::SUCCESS;
            }
            other => {
                if input.is_some() {
                    eprintln!("t27r: argumento a mais: {other}");
                    return ExitCode::from(2);
                }
                input = Some(other.to_string());
            }
        }
        i += 1;
    }
    let Some(input) = input else {
        eprintln!("uso: t27r <programa.t27r | -> [-o saida.tas]");
        return ExitCode::from(2);
    };
    let src = if input == "-" {
        let mut s = String::new();
        if let Err(e) = std::io::Read::read_to_string(&mut std::io::stdin(), &mut s) {
            eprintln!("t27r: não li a entrada padrão: {e}");
            return ExitCode::from(1);
        }
        s
    } else {
        match std::fs::read_to_string(&input) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("t27r: não li {input}: {e}");
                return ExitCode::from(1);
            }
        }
    };
    let (result, ext) = match mode {
        "pilha" => (lang::compile_pilha(&src).map(String::into_bytes), ".tas"),
        "emit-ir" => (lang::to_ir_mode(&src, kmode).map(|m| lang::ir::print(&m).into_bytes()), ".tir.txt"),
        "emit-tir" => (lang::to_ir_mode(&src, kmode).map(|m| m.encode()), ".tir"),
        _ => (lang::compile_mode(&src, kmode).map(String::into_bytes), ".tas"),
    };
    match result {
        Ok(tas) => {
            let out = output.unwrap_or_else(|| {
                if input == "-" {
                    "-".into()
                } else {
                    input.trim_end_matches(".t27r").to_string() + ext
                }
            });
            if out == "-" {
                if let Err(e) = std::io::Write::write_all(&mut std::io::stdout(), &tas) {
                    eprintln!("t27r: não escrevi a saída padrão: {e}");
                    return ExitCode::from(1);
                }
            } else if let Err(e) = std::fs::write(&out, tas) {
                eprintln!("t27r: não escrevi {out}: {e}");
                return ExitCode::from(1);
            } else {
                eprintln!("t27r: {input} → {out}");
            }
            ExitCode::SUCCESS
        }
        Err(d) => {
            eprintln!("{input}:{d}");
            ExitCode::from(1)
        }
    }
}
