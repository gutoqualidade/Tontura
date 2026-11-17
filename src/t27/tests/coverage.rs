//! TENERURA — cobertura de instruções do núcleo e medidas de custo.
//!
//! Um único teste roda todos os cenários-detectores, rodadas de caos e os
//! caminhos de pânico, e mede que fração das instruções do núcleo foi
//! executada. Instrução nunca executada = caminho do núcleo sem teste.
//! As medidas (custo de IPC em instruções, profundidade de pilha, tamanho)
//! vão para results/phase5/ quando T27_WRITE_RESULTS=1.

mod common;
use common::*;

use tenerura_t27::isa::disasm_word;
use tenerura_t27::machine::{Machine, Step, Stop, DEFAULT_RAM_HALF};
use tenerura_t27::sysimage::build_system;
use tenerura_t27::word::Word;

/// Instruções de núcleo e de usuário gastas, em média, por ida-e-volta
/// call/reply entre duas tarefas (diferença entre N e 2N chamadas).
fn ipc_round_trip_cost() -> (f64, f64) {
    let run = |n: i64| {
        let mut s = boot(vec![
            task("ping", with_word(user("ping"), "count", Word::lit(n))).send(0, 1, 7).con(),
            task("pong", user("pong")).recv(0, 1),
        ]);
        assert_eq!(s.m.run(50_000_000), Stop::Halted(0));
        (s.m.stats.instr_kernel as f64, s.m.stats.instr_user as f64, s.m.stats.traps_irq)
    };
    let (k1, u1, _) = run(2000);
    let (k2, u2, _) = run(4000);
    ((k2 - k1) / 2000.0, (u2 - u1) / 2000.0)
}

#[test]
fn kernel_instruction_coverage_and_costs() {
    let k = kernel();
    for (name, f) in detectors() {
        f(&k).unwrap_or_else(|e| panic!("detector '{name}': {e}"));
    }
    // caminhos de pânico
    {
        let mut s = boot(victims());
        let at = s.k("boot_table") + s.k("BT_MAGIC");
        s.m.poke(at, Word::lit(12345));
        assert_eq!(run_cov(&mut s.m, 100_000), Stop::Halted(-s.k("PANIC_BOOT")));
        let mut s = boot(victims());
        let at = s.k("boot_table") + s.k("BT_QUANTUM");
        s.m.poke(at, Word::lit(5));
        assert_eq!(run_cov(&mut s.m, 100_000), Stop::Halted(-s.k("PANIC_BOOT")), "fatia abaixo do mínimo");
        let mut s = boot(victims());
        let at = s.k("boot_table") + s.k("BT_NTASKS");
        s.m.poke(at, Word::lit(10));
        assert_eq!(run_cov(&mut s.m, 100_000), Stop::Halted(-s.k("PANIC_BOOT")), "tarefas demais");
        let mut s = boot(victims());
        let at = s.k("boot_table") + s.k("BT_NTASKS");
        s.m.poke(at, Word::lit(0));
        assert_eq!(run_cov(&mut s.m, 100_000), Stop::Halted(-s.k("PANIC_BOOT")), "zero tarefas");
        let img = build_system(&k, &[task("gordo", user("spin")).stack(200)]).unwrap();
        let mut m = Machine::new(40 * 729 + 364);
        img.load(&mut m).unwrap();
        assert_eq!(run_cov(&mut m, 2_000_000), Stop::Halted(-img.ksym("PANIC_OOM")));
        let mut s = boot(victims());
        let at = s.k("do_syscall");
        s.m.poke(at, Word::ZERO);
        assert_eq!(run_cov(&mut s.m, 2_000_000), Stop::Halted(-s.k("PANIC_NESTED")));
        // interrupção de linha inesperada: o núcleo mascara e segue
        let mut s = boot(victims());
        while s.m.step() != Step::Eret {}
        s.m.imask |= 0b100;
        s.m.irq_ext |= 0b100;
        assert_eq!(run_cov(&mut s.m, 2_000_000), Stop::Halted(0));
        assert_eq!(s.m.imask, 1, "linha sem driver ficou mascarada");
        assert_victims_ok(&s, "linha de interrupção inesperada");
    }

    let cov = KERNEL_COV.lock().unwrap().clone();
    let (t0, t1) = (k.sym("__text_start"), k.sym("__text_end"));
    let total = (t0..t1).count();
    let missed: Vec<i64> = (t0..t1).filter(|a| !cov.contains(a)).collect();
    let pct = 100.0 * (total - missed.len()) as f64 / total as f64;
    let mut report = format!(
        "núcleo: {total} palavras de texto, {} de dados; cobertura de instruções {:.2}% ({} não executadas)\n",
        k.sym("kernel_end") - k.sym("__data_start"),
        pct,
        missed.len()
    );
    for a in &missed {
        let (_, w, src) = k.listing.iter().find(|(addr, _, _)| addr == a).unwrap();
        report += &format!("  não executada {a:>5}: {:<28} | {src}\n", disasm_word(*w, Some(*a)));
    }
    let (ipc_k, ipc_u) = ipc_round_trip_cost();
    let kstack = *KSTACK_MAX.lock().unwrap();
    report += &format!("IPC call/reply (ida e volta): {ipc_k:.1} instruções de núcleo + {ipc_u:.1} de usuário\n");
    report += &format!("pilha do núcleo: profundidade máxima observada {kstack} palavras\n");
    let (burst, burst_restart) = *BURST_MAX.lock().unwrap();
    report += &format!(
        "maior trecho não preemptível do núcleo (trap → retorno): {burst} passos; com reinício de tarefa: {burst_restart} passos\n"
    );
    let _ = DEFAULT_RAM_HALF;
    eprint!("{report}");
    if std::env::var("T27_WRITE_RESULTS").is_ok() {
        let out = os_dir().join("../../results/phase5/kernel-coverage.txt");
        std::fs::write(out, &report).unwrap();
    }
    // As únicas instruções aceitas como não executadas são os laços de
    // segurança depois de desligar a máquina e o `brk` das verificações de
    // limite que o TIR põe no código gerado (docs/14 §6): inalcançáveis
    // enquanto os chamadores respeitam o contrato — e, se um deles falhar,
    // o núcleo para em pânico em vez de escrever fora do array.
    for a in &missed {
        let (_, _, src) = k.listing.iter().find(|(addr, _, _)| addr == a).unwrap();
        assert!(
            src.contains("j 9b") || src.contains("j 1b") || src.contains(".Ltrap_"),
            "caminho do núcleo sem teste em {a}: {src}"
        );
    }
}
