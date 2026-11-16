//! TENERURA — testes de sistema do micronúcleo v1: boot, IPC, prazos,
//! preempção, contenção de falhas, supervisão e driver em espaço de
//! usuário, observados de fora (console + tabela de tarefas lida da RAM),
//! com os invariantes do núcleo conferidos a cada saída do núcleo.
//! Especificação: docs/08-micronucleo.md. Os cenários vivem em
//! tests/common (são os mesmos usados como detectores nos testes de mutação).

mod common;
use common::*;

use tenerura_t27::isa::cause;
use tenerura_t27::machine::{Machine, Stop};
use tenerura_t27::sysimage::build_system;
use tenerura_t27::word::Word;

macro_rules! scenarios {
    ($($name:ident => $f:ident;)*) => {
        $( #[test] fn $name() { $f(&kernel()).unwrap_or_else(|e| panic!("{e}")); } )*
    };
}

scenarios! {
    boots_and_two_tasks_communicate => sc_ping_pong;
    preemption_keeps_contexts_and_lets_victims_finish => sc_preempt;
    every_fault_class_kills_only_the_offender => sc_faults;
    server_death_wakes_blocked_caller_with_error => sc_dead_peer;
    invalid_requests_are_refused_not_fatal => sc_invalid_requests;
    messages_arrive_intact_in_fifo_order_with_badges => sc_fifo_badges;
    capabilities_cannot_be_forged => sc_cap_forgery;
    sleepers_wake_in_deadline_order_and_oversized_sleep_is_clamped => sc_sleep;
    clock_wraparound_is_harmless => sc_clock_wrap;
    ipc_timeouts_bound_every_wait => sc_timeouts;
    replies_can_be_deferred_and_receipts_are_single_use => sc_deferred_reply;
    receipts_are_bound_to_the_receiving_server => sc_reply_forgery;
    supervisor_restarts_failing_service_and_client_recovers => sc_supervision;
    restarted_task_is_indistinguishable_from_fresh => sc_restart_is_fresh;
    kill_and_restart_wake_dependents => sc_kill_and_restart_wake_dependents;
    user_level_driver_gets_interrupts_as_messages => sc_driver_irq;
    killed_driver_is_restarted_and_service_resumes => sc_driver_restart;
    receivers_are_served_in_arrival_order => sc_fifo_receivers;
    pending_death_notices_are_collected_once_in_order => sc_zombie_notices;
}

#[test]
fn fault_in_one_task_does_not_compromise_the_others() {
    // ping/pong + uma tarefa que falha + uma que nunca cede o processador
    let mut tasks = vec![task("crash", user("crash")).con(), task("spin", user("spin"))];
    tasks.extend(victims());
    let mut s = boot(tasks);
    // spin nunca termina: o sistema não desliga sozinho
    assert_eq!(run_checked(&mut s, 3_000_000, 64, |_| false).unwrap(), Stop::StepLimit);
    let out = s.m.console_text();
    assert!(out.contains("ping: soma=65\n"), "saída: {out:?}");
    assert!(out.contains('x') && !out.contains('y'));
    let crash = s.task("crash");
    assert_eq!((crash.state, crash.fcause, crash.fval), (s.k("ST_DEAD"), cause::STORE_FAULT, 0));
    assert_victims_ok(&s, "demo 1");
    let spin = s.task("spin");
    assert_eq!(spin.state, s.k("ST_READY"));
    assert!(spin.reg(-1) > 1000, "spin continuou rodando (s1 = {})", spin.reg(-1));
}

#[test]
fn context_survives_thousands_of_preemptions() {
    // fatia mínima: a tarefa é interrompida em pontos arbitrários
    let tasks = vec![
        task("k1", reg_keeper(1_000_003, 3000)),
        task("k2", reg_keeper(-77_777_777, 3000)),
        task("k3", reg_keeper(152_415_787_501, 3000)),
        task("spin", user("spin")),
    ];
    let mut s = boot_full(tasks, 200, Some(0xC0FFEE));
    let dead = s.k("ST_DEAD");
    let (stop, obs) = run_observed(&mut s, 30_000_000, 4096, |s| {
        ["k1", "k2", "k3"].iter().all(|n| s.task(n).state == dead)
    })
    .unwrap();
    assert_eq!(stop, Stop::StepLimit);
    for n in ["k1", "k2", "k3"] {
        assert_eq!((s.task(n).exit, s.task(n).fcause), (0, 0), "{n}");
    }
    assert!(s.m.stats.traps_irq > 3000, "preempções: {}", s.m.stats.traps_irq);
    assert!(obs.kernel_exits > 3000);
}

#[test]
fn new_task_starts_with_clean_registers() {
    // nenhum valor do núcleo ou lixo do boot vaza para a tarefa nova:
    // todos os registradores zero, exceto sp
    let regs = ["a1", "a2", "a3", "a4", "a5", "a6", "a7", "a8", "a9", "t1", "t2", "t3", "t4", "s1", "s2", "s3",
        "s4", "s5", "s6", "s7", "s8", "s9", "s10", "s11", "lr"];
    let mut body = String::new();
    for r in regs {
        body += &format!("    bnz {r}, sujo\n");
    }
    body += "    li t1, USER_SP\n    bne sp, t1, sujo\n    li a2, 0\n    j exit\nsujo:\n    li a2, 1\n    j exit\n";
    let mut s = boot_full(vec![task("t", prog(&body))], 0, Some(0xBAD_5EED));
    assert_eq!(run_checked(&mut s, 500_000, 1, |_| false).unwrap(), Stop::Halted(0));
    assert_eq!((s.task("t").exit, s.task("t").fcause), (0, 0));
}

#[test]
fn garbage_ram_at_boot_changes_nothing() {
    // O núcleo não pode depender de memória zerada: mesma execução com a
    // RAM e os registradores cheios de lixo antes da carga da imagem.
    let run = |garbage: Option<u64>| {
        let mut s = boot_full(victims(), 0, garbage);
        let stop = run_checked(&mut s, 2_000_000, 16, |_| false).unwrap();
        (stop, s.m.console_text(), s.m.time, s.m.stats.instr_user, s.m.stats.instr_kernel)
    };
    let clean = run(None);
    assert_eq!(clean.0, Stop::Halted(0));
    for seed in [1u64, 0xDEAD_BEEF, 0x5EED_2026_1116] {
        // determinismo total: mesmo número de passos e de instruções
        assert_eq!(run(Some(seed)), clean, "lixo com semente {seed:#x}");
    }
}

#[test]
fn lone_receiver_blocks_forever_and_system_reports_it() {
    let mut s = boot(vec![task("t", prog("li a1, SYS_RECV\n li a2, 0\n li t1, FOREVER\n sys\n li a2, 0\n j exit")).recv(0, 1)]);
    assert_eq!(run_checked(&mut s, 500_000, 1, |_| false).unwrap(), Stop::Halted(3));
    assert_eq!(s.task("t").state, s.k("ST_BLK_RECV"));
}

// ───────────────────────── falhas do próprio núcleo ─────────────────────────

#[test]
fn kernel_refuses_bad_boot_table_and_out_of_memory() {
    // magia errada
    let mut s = boot(victims());
    let magic_at = s.k("boot_table") + s.k("BT_MAGIC");
    s.m.poke(magic_at, Word::lit(12345));
    assert_eq!(s.m.run(100_000), Stop::Halted(-s.k("PANIC_BOOT")));
    assert_eq!(s.m.console_text(), "!K\n");
    // RAM pequena demais para as pilhas pedidas
    let img = build_system(&kernel(), &[task("gordo", user("spin")).stack(200)]).unwrap();
    let mut m = Machine::new(40 * 729 + 364);
    img.load(&mut m);
    assert_eq!(m.run(2_000_000), Stop::Halted(-img.ksym("PANIC_OOM")));
}

#[test]
fn kernel_fault_is_fail_stop() {
    // Injeta uma instrução ilegal no caminho de chamadas do núcleo: o trap
    // nasce em modo núcleo, a entrada de trap percebe e desliga (pânico)
    // em vez de seguir com estado corrompido.
    let mut s = boot(victims());
    let at = s.k("do_syscall");
    s.m.poke(at, Word::ZERO);
    assert_eq!(s.m.run(2_000_000), Stop::Halted(-s.k("PANIC_NESTED")));
    assert_eq!(s.m.peek(s.k("panic_code")).unwrap().val(), s.k("PANIC_NESTED"));
}

#[test]
fn image_builder_rejects_inconsistent_configurations() {
    use tenerura_t27::machine::DEV_CONSOLE;
    let k = kernel();
    let bad: Vec<(&str, Vec<tenerura_t27::sysimage::TaskSpec>)> = vec![
        ("nenhuma tarefa", vec![]),
        ("dez tarefas", (0..10).map(|i| task(&format!("t{i}"), user("spin"))).collect()),
        ("capacidade para tarefa inexistente", vec![task("a", user("spin")).cap(0, CAP_TASK, 5, 0)]),
        ("duas tarefas com o mesmo dispositivo", vec![
            task("a", user("spin")).device(DEV_CONSOLE, 100),
            task("b", user("spin")).device(DEV_CONSOLE, 100),
        ]),
        ("dispositivo de energia concedido", vec![task("a", user("spin")).device(DEV_CONSOLE + 729, 100)]),
        ("dispositivo sobre a página do código", vec![task("a", user("spin")).device(DEV_CONSOLE, 1)]),
        ("dois drivers na mesma linha", vec![task("a", user("spin")).irq(2, 1, -1), task("b", user("spin")).irq(2, 1, -1)]),
        ("linha do timer como dispositivo", vec![task("a", user("spin")).irq(1, 1, -1)]),
        ("slot inexistente", vec![task("a", user("spin")).cap(9, CAP_EP_SEND, 1, 0)]),
        ("pilha vazia", vec![task("a", user("spin")).stack(0)]),
        ("selo de aviso não negativo", vec![task("a", user("spin")).fault(20, 1)]),
        ("selo de interrupção não negativo", vec![task("a", user("spin")).irq(2, 10, 0)]),
        ("selo negativo em capacidade de envio", vec![task("a", user("spin")).cap(0, CAP_EP_SEND, 20, -1)]),
    ];
    for (what, tasks) in bad {
        assert!(build_system(&k, &tasks).is_err(), "o construtor aceitou: {what}");
    }
}
