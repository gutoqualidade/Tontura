//! TENERURA — ferramenta de linha de comando da máquina T27.
//!
//!   t27 asm <arquivo.tas>            monta e mostra a listagem
//!   t27 run <arquivo.tas> [passos]   monta e executa em modo núcleo (máquina nua)
//!   t27 sys <manifesto> [passos] [--input "texto|mais texto"]
//!                                    constrói o sistema (núcleo + tarefas) e executa;
//!                                    cada trecho de --input (separado por |) é "digitado"
//!                                    no console quando o sistema fica ocioso

use std::path::Path;
use std::process::ExitCode;

use tenerura_t27::asm::assemble_file;
use tenerura_t27::isa::{cause, disasm_word};
use tenerura_t27::machine::{Machine, Stop, DEFAULT_RAM_HALF};
use tenerura_t27::manifest::load_manifest;
use tenerura_t27::sysimage::{state_name, task_table};
use tenerura_t27::word::Word;

fn usage() -> ExitCode {
    eprintln!("uso: t27 asm <arquivo.tas> | t27 run <arquivo.tas> [passos] | t27 sys <manifesto> [passos] [--input texto]");
    ExitCode::from(2)
}

fn report(m: &Machine, stop: Stop) {
    print!("{}", m.console_text());
    eprintln!("── parada: {stop:?}");
    eprintln!(
        "── passos: {}  instruções: usuário {}  núcleo {}  traps: falha {}  chamada {}  irq {}",
        m.time, m.stats.instr_user, m.stats.instr_kernel, m.stats.traps_fault, m.stats.traps_syscall, m.stats.traps_irq
    );
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        return usage();
    }
    let steps: u64 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(10_000_000);
    match args[1].as_str() {
        "asm" => match assemble_file(Path::new(&args[2])) {
            Ok(p) => {
                for (addr, w, src) in &p.listing {
                    println!("{addr:>9}  {:>15}  {:<28} | {src}", w.val(), disasm_word(*w, Some(*addr)));
                }
                eprintln!("── entrada: {}  palavras: {}", p.entry, p.listing.len());
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("erro: {e}");
                ExitCode::FAILURE
            }
        },
        "run" => match assemble_file(Path::new(&args[2])) {
            Ok(p) => {
                let mut m = Machine::new(DEFAULT_RAM_HALF);
                if let Err(e) = p.segments.iter().try_for_each(|s| m.load(s.base, &s.words)) {
                    eprintln!("erro: {e}");
                    return ExitCode::FAILURE;
                }
                m.pc = Word::lit(p.entry);
                let stop = m.run(steps);
                report(&m, stop);
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("erro: {e}");
                ExitCode::FAILURE
            }
        },
        "sys" => match load_manifest(Path::new(&args[2])) {
            Ok(img) => {
                let mut m = Machine::new(DEFAULT_RAM_HALF);
                if let Err(e) = img.load(&mut m) {
                    eprintln!("erro: {e}");
                    return ExitCode::FAILURE;
                }
                let input = args.iter().position(|a| a == "--input").and_then(|i| args.get(i + 1)).cloned();
                let mut chunks: Vec<&str> = input.as_deref().map(|s| s.split('|').collect()).unwrap_or_default();
                chunks.reverse();
                let mut stop = m.run(steps);
                // ocioso = só uma interrupção externa acorda a máquina: "digita" o próximo trecho
                while stop == Stop::Idle {
                    let Some(chunk) = chunks.pop() else { break };
                    m.console_in.extend(chunk.chars().map(|c| c as i64));
                    stop = m.run(steps);
                }
                report(&m, stop);
                eprintln!("── tarefas:");
                for t in task_table(&img, &m) {
                    let how = if t.state != img.ksym("ST_DEAD") {
                        String::new()
                    } else if t.fcause == 0 {
                        format!("saiu com código {}", t.exit)
                    } else if t.fcause == img.ksym("CAUSE_KILLED") {
                        "morta por capacidade de tarefa".to_string()
                    } else {
                        format!("FALHOU: {} (tval {}, pc {})", cause::name(t.fcause), t.fval, t.pc)
                    };
                    eprintln!("   {} {:<10} {:<14} geração {}  {how}", t.id, t.name, state_name(&img, t.state), t.gen);
                }
                let panic = m.peek(img.ksym("panic_code")).map_or(0, |w| w.val());
                if panic != 0 {
                    eprintln!("── PÂNICO DO NÚCLEO: código {panic}");
                    return ExitCode::FAILURE;
                }
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("erro: {e}");
                ExitCode::FAILURE
            }
        },
        _ => usage(),
    }
}
