//! TENERURA — testes de CAOS: tarefas hostis (chamadas de sistema
//! aleatórias com argumentos aleatórios; código aleatório; e que se matam
//! e reiniciam umas às outras) rodam ao lado de um par cliente/servidor
//! bem-comportado.
//!
//! Propriedade testada (contenção): tarefas SEM capacidade para o canal das
//! vítimas não conseguem (a) derrubar o núcleo, (b) violar um invariante do
//! núcleo, (c) impedir as vítimas de concluir com o resultado correto,
//! (d) escrever no console. Cada rodada varia: sementes, ordem das
//! tarefas, fatia de tempo e lixo inicial na RAM. Sementes registradas.
//! Limitação: é amostragem do espaço de comportamentos, não prova.

mod common;
use common::*;

use tenerura_t27::machine::Stop;

const SEED: u64 = 0x5EED_2026_1116 ^ 0xCA05;

fn chaos_round(round: u64, n: i64) -> Observed {
    let mut rng = XorShift(SEED.wrapping_add(round.wrapping_mul(0x9E37_79B9_7F4A_7C15)) | 1);
    let (tasks, _) = chaos_tasks(&mut rng, n);
    let quantum = rng.range(200, 6000);
    let garbage = rng.next();
    let mut s = boot_full(tasks, quantum, Some(garbage));
    let dead = s.k("ST_DEAD");
    let (stop, obs) = run_observed(&mut s, 8_000_000, 256, |s| {
        s.task("ping").state == dead && s.task("pong").state == dead
    })
    .unwrap_or_else(|e| panic!("rodada {round} (quantum {quantum}): {e}"));
    let ctx = format!("rodada {round} quantum {quantum} parada {stop:?}");
    // só o ping tem a capacidade de console: a saída é EXATAMENTE a dele
    assert_eq!(s.m.console_text(), format!("ping: soma={}\n", n * (n + 1) / 2 + n), "{ctx}");
    for name in ["ping", "pong"] {
        let t = s.task(name);
        assert_eq!((t.state, t.exit, t.fcause, t.gen), (dead, 0, 0, 0), "{ctx}: {name}");
    }
    assert_eq!(stop, Stop::StepLimit, "{ctx}: parou ao concluir as vítimas");
    obs
}

#[test]
fn hostile_tasks_cannot_break_kernel_or_victims() {
    let rounds = 200;
    let mut total = Observed::default();
    for r in 0..rounds {
        total.merge(&chaos_round(r, 150));
    }
    let calls: u64 = total.syscalls.iter().sum();
    let faults: u64 = total.faults.iter().sum();
    let mut report = format!(
        "caos: {rounds} rodadas, {calls} chamadas de sistema, {faults} falhas contidas, {} reinícios pedidos, {} saídas do núcleo verificadas\n",
        total.restarts, total.kernel_exits
    );
    report += &format!("caos: chamadas por número (último = inválido): {:?}\n", total.syscalls);
    report += &format!("caos: falhas por causa (índice = −causa): {:?}\n", total.faults);
    eprint!("{report}");
    if std::env::var("T27_WRITE_RESULTS").is_ok() {
        std::fs::write(os_dir().join("../../results/phase5/chaos.txt"), &report).unwrap();
    }
    // o teste só vale se o caos de fato aconteceu: toda chamada e toda
    // classe de falha alcançável por usuário foi exercitada muitas vezes
    assert!(total.syscalls.iter().all(|&c| c > 500), "{:?}", total.syscalls);
    for c in 1..=6 {
        assert!(total.faults[c] > 0, "causa −{c} nunca ocorreu");
    }
    assert!(total.restarts > 50, "poucos reinícios: {}", total.restarts);
}
