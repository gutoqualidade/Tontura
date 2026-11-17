//! TENERURA — exploração EXAUSTIVA LIMITADA do núcleo real.
//!
//! Os testes de caos amostram; este enumera. Três tarefas que só executam
//! `sys` em laço são conduzidas pelo arnês: a cada vez que o núcleo entrega
//! o processador a uma tarefa, o arnês escolhe a próxima ação dela num
//! cardápio (enviar, receber, chamar, responder, dormir, sair, falhar, ser
//! preemptada, matar/reiniciar outra) e TODAS as sequências de até
//! `T27_DEPTH` ações (padrão 4) são executadas no núcleo de verdade
//! (assembly ternário no emulador), com os invariantes conferidos a cada
//! saída do núcleo.
//!
//! O que isto dá: nenhum invariante é violado por NENHUMA sequência do
//! cardápio até a profundidade explorada (hipótese do escopo pequeno).
//! O que não dá: nada além dessa profundidade, desse cardápio e dessas 3
//! tarefas — continua sendo teste, não prova.

mod common;
use common::*;

use std::collections::HashSet;

use tenerura_t27::kcheck::KCheck;
use tenerura_t27::machine::{Machine, Step};
use tenerura_t27::sysimage::{build_system, task_table, SystemImage};
use tenerura_t27::word::Word;

const SYS_AT: i64 = 365; // endereço do `sys` do programa-toco
const N: usize = 3;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Act {
    Yield,
    Send(i64),
    Recv(i64),
    Call(i64),
    Reply,
    Sleep,
    Exit,
    Kill,
    Restart,
    Fault,
    Preempt,
}

const MENU: [Act; 14] = [
    Act::Yield,
    Act::Send(-1),
    Act::Send(0),
    Act::Recv(-1),
    Act::Recv(0),
    Act::Call(-1),
    Act::Call(300),
    Act::Reply,
    Act::Sleep,
    Act::Exit,
    Act::Kill,
    Act::Restart,
    Act::Fault,
    Act::Preempt,
];

#[derive(Clone)]
struct Node {
    m: Machine,
    /// último recibo que cada tarefa recebeu num `recv`
    handles: [i64; N],
    last: [Option<Act>; N],
}

struct Explorer {
    img: SystemImage,
    kc: KCheck,
    free_start: i64,
    nodes: u64,
    terminals: u64,
    exits: u64,
    states: HashSet<Vec<i64>>,
}

impl Explorer {
    fn k(&self, s: &str) -> i64 {
        self.img.ksym(s)
    }

    fn cur_task(&self, m: &Machine) -> usize {
        let cur = m.peek(self.k("cur")).unwrap().val();
        m.peek(cur + self.k("T_ID")).unwrap().val() as usize
    }

    /// Avança até a próxima escolha (tarefa em modo usuário parada no `sys`).
    /// `false` = a máquina parou (desligou ou ficou ociosa para sempre).
    fn settle(&mut self, m: &mut Machine, path: &[Act]) -> bool {
        for _ in 0..400_000 {
            if !m.kernel && m.pc.val() == SYS_AT {
                return true;
            }
            match m.step() {
                Step::Exec | Step::Trap(_) => {}
                Step::Eret => {
                    self.exits += 1;
                    m.watch_from = Some(self.free_start - 364);
                    self.kc.check_state(&self.img, m, true).unwrap_or_else(|e| panic!("{e}\ncaminho: {path:?}"));
                }
                Step::Halted(code) => {
                    assert!(code >= 0, "pânico do núcleo ({code}); caminho: {path:?}");
                    return false;
                }
                Step::Idle => return false,
                Step::DoubleFault => panic!("falha dupla; caminho: {path:?}"),
            }
        }
        panic!("o núcleo não devolveu o processador; caminho: {path:?}");
    }

    /// Assinatura abstrata do estado do núcleo (para contar estados distintos).
    fn signature(&self, m: &Machine) -> Vec<i64> {
        let tasks = task_table(&self.img, m);
        let idx = |tcb: i64| tasks.iter().position(|t| t.tcb == tcb).map_or(-1, |i| i as i64);
        let mut v = vec![idx(m.peek(self.k("cur")).unwrap().val())];
        for t in &tasks {
            v.extend([t.state, t.ep, idx(t.peer), t.timed, t.sendkind, t.gen.min(2)]);
        }
        v
    }

    fn explore(&mut self, node: &Node, depth: usize, path: &mut Vec<Act>) {
        self.nodes += 1;
        self.states.insert(self.signature(&node.m));
        if depth == 0 {
            return;
        }
        for act in MENU {
            let mut n = node.clone();
            let t = self.cur_task(&n.m);
            // recibo entregue pelo `recv` anterior desta tarefa
            if matches!(n.last[t], Some(Act::Recv(_))) && n.m.reg(1).val() == 1 && n.m.reg(10).val() != 0 {
                n.handles[t] = n.m.reg(10).val();
            }
            n.last[t] = Some(act);
            let set = |m: &mut Machine, r: i8, v: i64| m.set_reg(r, Word::lit(v));
            for r in 3..=9 {
                set(&mut n.m, r, 1000 * (t as i64 + 1) + r as i64);
            }
            let sys = |m: &mut Machine, no: i64, a2: i64, t1: i64| {
                m.set_reg(1, Word::lit(no));
                m.set_reg(2, Word::lit(a2));
                m.set_reg(10, Word::lit(t1));
            };
            match act {
                Act::Yield => sys(&mut n.m, self.k("SYS_YIELD"), 0, 0),
                Act::Send(p) => sys(&mut n.m, self.k("SYS_SEND"), 0, p),
                Act::Recv(p) => sys(&mut n.m, self.k("SYS_RECV"), 1, p),
                Act::Call(p) => sys(&mut n.m, self.k("SYS_CALL"), 0, p),
                Act::Reply => sys(&mut n.m, self.k("SYS_REPLY"), n.handles[t], 0),
                Act::Sleep => sys(&mut n.m, self.k("SYS_SLEEP"), 300, 0),
                Act::Exit => sys(&mut n.m, self.k("SYS_EXIT"), 7, 0),
                Act::Kill | Act::Restart => {
                    sys(&mut n.m, self.k("SYS_TASK"), 2, 0);
                    set(&mut n.m, 3, if act == Act::Kill { -1 } else { 1 });
                    n.m.watch_from = None; // o reinício reconstrói um espaço de endereços
                }
                Act::Fault => n.m.pc = Word::ZERO, // página 0: falha de busca
                Act::Preempt => n.m.timecmp = Word::wrap(n.m.time), // o timer vence agora
            }
            path.push(act);
            // sai do ponto de escolha (executa o `sys`, a falha ou a interrupção)…
            match n.m.step() {
                Step::Trap(_) => {}
                other => panic!("esperava um trap, veio {other:?}; caminho: {path:?}"),
            }
            // …e roda o núcleo até a próxima escolha
            if self.settle(&mut n.m, path) {
                self.explore(&n, depth - 1, path);
            } else {
                self.terminals += 1;
                self.kc.check_state(&self.img, &n.m, false).unwrap_or_else(|e| panic!("{e}\ncaminho: {path:?}"));
            }
            if depth == 1 {
                self.kc.check_memory(&self.img, &n.m).unwrap_or_else(|e| panic!("{e}\ncaminho: {path:?}"));
                assert_eq!(n.m.watch_hits, 0, "o núcleo tocou memória de usuário; caminho: {path:?}");
                assert_eq!(n.m.peek(self.k("panic_code")).unwrap().val(), 0, "pânico; caminho: {path:?}");
            }
            path.pop();
        }
    }
}

#[test]
fn every_short_action_sequence_keeps_the_kernel_invariants() {
    let depth: usize = std::env::var("T27_DEPTH").ok().and_then(|s| s.parse().ok()).unwrap_or(4);
    let stub = || user_src("    .include \"../lib/abi.inc\"\n    .org USER_TEXT\n_start:\n    sys\n    j _start\n");
    // todos enviam e recebem no endpoint 1; t0 tem capacidade de tarefa sobre
    // t1; a morte de t1 vira aviso no mesmo endpoint (caminho do zumbi)
    let tasks = vec![
        task("t0", stub()).send(0, 1, 10).recv(1, 1).cap(2, CAP_TASK, 1, 0),
        task("t1", stub()).send(0, 1, 11).recv(1, 1).fault(1, -1),
        task("t2", stub()).send(0, 1, 12).recv(1, 1),
    ];
    let img = build_system(&kernel(), &tasks).unwrap();
    // RAM pequena: cada nó da busca copia a máquina inteira
    let mut m = Machine::new(30 * 729 + 364);
    img.load(&mut m);
    let kc = KCheck::new(&img, &m);
    let free_start = m.peek(img.ksym("boot_table") + img.ksym("BT_FREE")).unwrap().val();
    let mut ex = Explorer { img, kc, free_start, nodes: 0, terminals: 0, exits: 0, states: HashSet::new() };
    assert!(ex.settle(&mut m, &[]), "o sistema não chegou à primeira tarefa");
    let root = Node { m, handles: [0; N], last: [None; N] };
    ex.explore(&root, depth, &mut Vec::new());

    let report = format!(
        "exploração exaustiva limitada: profundidade {depth}, cardápio de {} ações, {N} tarefas\n\
         sequências executadas (nós): {}  terminais (sistema parou): {}\n\
         saídas do núcleo com invariantes conferidos: {}  estados abstratos distintos do núcleo: {}\n\
         violações: 0\n",
        MENU.len(),
        ex.nodes,
        ex.terminals,
        ex.exits,
        ex.states.len()
    );
    eprint!("{report}");
    if std::env::var("T27_WRITE_RESULTS").is_ok() {
        std::fs::write(os_dir().join("../../results/phase5/exhaustive.txt"), &report).unwrap();
    }
    assert!(ex.states.len() > 100, "exploração rasa demais: {} estados", ex.states.len());
}
