//! Apoio comum aos testes de sistema: monta o núcleo e as tarefas a partir
//! de src/os, roda com verificação de invariantes e oferece os
//! cenários-detectores usados também nos testes de mutação.
#![allow(dead_code)]

use std::path::PathBuf;

use tenerura_t27::asm::{assemble_file, assemble_str, Perm, Program, Segment};
use tenerura_t27::isa::{encode, Fmt, Instr, Op, IMM16_MAX, IMM19_MAX, LUI_MAX};
use tenerura_t27::kcheck::KCheck;
use tenerura_t27::machine::{Machine, Step, Stop, DEFAULT_RAM_HALF, DEV_CONSOLE};
use tenerura_t27::sysimage::{build_system_with, task_table, SystemImage, TaskSpec, TaskView};
pub use tenerura_t27::sysimage::{CAP_CONSOLE, CAP_EP_RECV, CAP_EP_SEND, CAP_IRQ, CAP_TASK};
use tenerura_t27::word::Word;

pub fn os_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../os")
}

/// O núcleo de src/os — ou o de `T27_KERNEL`, para rodar a suíte contra um
/// núcleo alterado (controle negativo manual).
pub fn kernel() -> Program {
    let path = std::env::var("T27_KERNEL").map(PathBuf::from).unwrap_or_else(|_| os_dir().join("kernel/kernel.tas"));
    assemble_file(&path).unwrap_or_else(|e| panic!("núcleo: {e}"))
}

/// Programa de usuário de src/os/user/<nome>.tas.
pub fn user(name: &str) -> Program {
    assemble_file(&os_dir().join(format!("user/{name}.tas"))).unwrap_or_else(|e| panic!("{name}: {e}"))
}

/// Programa de usuário a partir de texto (os `.include` resolvem em src/os/user).
pub fn user_src(src: &str) -> Program {
    assemble_str(src, "inline", Some(&os_dir().join("user"))).unwrap_or_else(|e| panic!("{e}\n{src}"))
}

/// Programa de usuário: `corpo` entre o cabeçalho padrão (ABI, origem,
/// `_start`) e a biblioteca (saída pelo console de depuração).
pub fn prog(body: &str) -> Program {
    user_src(&format!(
        "    .include \"../lib/abi.inc\"\n    .org USER_TEXT\n_start:\n{body}\n    .text\n    .include \"../lib/ulib.inc\"\n"
    ))
}

/// Idem, com a biblioteca que fala com o servidor de console.
pub fn prog_drv(body: &str) -> Program {
    user_src(&format!(
        "    .include \"../lib/abi.inc\"\n    .org USER_TEXT\n_start:\n{body}\n    .text\n    .include \"../lib/ulib_drv.inc\"\n"
    ))
}

pub fn task(name: &str, program: Program) -> TaskSpec {
    TaskSpec::new(name, program)
}

/// Atalhos de capacidade.
pub trait Caps {
    fn send(self, slot: usize, ep: i64, badge: i64) -> Self;
    fn recv(self, slot: usize, ep: i64) -> Self;
    fn con(self) -> Self;
}

impl Caps for TaskSpec {
    fn send(self, slot: usize, ep: i64, badge: i64) -> Self {
        self.cap(slot, CAP_EP_SEND, ep, badge)
    }
    fn recv(self, slot: usize, ep: i64) -> Self {
        self.cap(slot, CAP_EP_RECV, ep, 0)
    }
    /// console de depuração no slot convencional da biblioteca
    fn con(self) -> Self {
        self.cap(8, CAP_CONSOLE, 0, 0)
    }
}

pub struct Sys {
    pub m: Machine,
    pub img: SystemImage,
}

impl Sys {
    pub fn tasks(&self) -> Vec<TaskView> {
        task_table(&self.img, &self.m)
    }

    pub fn task(&self, name: &str) -> TaskView {
        self.tasks()
            .into_iter()
            .find(|t| t.name == name)
            .unwrap_or_else(|| panic!("tarefa {name} não existe"))
    }

    pub fn k(&self, sym: &str) -> i64 {
        self.img.ksym(sym)
    }

    /// Entrega texto ao dispositivo de console (como um teclado).
    pub fn type_text(&mut self, text: &str) {
        self.m.console_in.extend(text.chars().map(|c| c as i64));
    }
}

/// Sistema com o núcleo `k`. `quantum` 0 = padrão; `garbage` = semente
/// para encher RAM e registradores de lixo antes da carga.
pub fn boot_k(k: &Program, tasks: Vec<TaskSpec>, quantum: i64, garbage: Option<u64>) -> Result<Sys, String> {
    let img = build_system_with(k, &tasks, quantum)?;
    let mut m = Machine::new(DEFAULT_RAM_HALF);
    if let Some(seed) = garbage {
        m.randomize(seed);
    }
    img.load(&mut m).unwrap();
    Ok(Sys { m, img })
}

pub fn boot_full(tasks: Vec<TaskSpec>, quantum: i64, garbage: Option<u64>) -> Sys {
    boot_k(&kernel(), tasks, quantum, garbage).unwrap_or_else(|e| panic!("imagem: {e}"))
}

pub fn boot(tasks: Vec<TaskSpec>) -> Sys {
    boot_full(tasks, 0, None)
}

/// PRNG xorshift64* (o mesmo das fases anteriores), sementes registradas.
pub struct XorShift(pub u64);

impl XorShift {
    pub fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    /// uniforme em lo..=hi
    pub fn range(&mut self, lo: i64, hi: i64) -> i64 {
        lo + (self.next() % (hi - lo + 1) as u64) as i64
    }
    pub fn word(&mut self) -> Word {
        Word::wrap128(self.next() as i128)
    }
}

/// Troca a palavra no símbolo `sym` do segmento de dados do programa.
pub fn with_word(mut p: Program, sym: &str, v: Word) -> Program {
    let addr = p.sym(sym);
    for s in p.segments.iter_mut() {
        if addr >= s.base && addr < s.end() {
            s.words[(addr - s.base) as usize] = v;
            return p;
        }
    }
    panic!("símbolo {sym} fora dos segmentos");
}

/// Programa de usuário feito de instruções VÁLIDAS aleatórias (inclusive
/// privilegiadas, `sys` e `brk`) e algumas palavras cruas: comportamento
/// arbitrário de uma tarefa.
pub fn random_code(rng: &mut XorShift, n: usize) -> Program {
    let mut words = Vec::with_capacity(n);
    while words.len() < n {
        if rng.next() % 50 == 0 {
            words.push(rng.word()); // palavra crua: quase sempre instrução ilegal
            continue;
        }
        let op = Op::ALL[rng.range(0, Op::ALL.len() as i64 - 1) as usize];
        let r = |rng: &mut XorShift| rng.range(-13, 13) as i8;
        let imm = |rng: &mut XorShift, max: i64| {
            if rng.next() % 2 == 0 {
                rng.range(-40, 40)
            } else {
                rng.range(-max, max)
            }
        };
        let ins = match op.fmt() {
            Fmt::R => Instr::r(op, r(rng), r(rng), r(rng)),
            Fmt::I => match op {
                Op::Lui => Instr::i(op, r(rng), 0, imm(rng, LUI_MAX)),
                Op::Csrr => Instr::i(op, r(rng), 0, rng.range(0, 13)),
                Op::Csrw => Instr::i(op, 0, r(rng), rng.range(0, 13)),
                Op::Csrx => Instr::i(op, r(rng), r(rng), rng.range(0, 13)),
                _ => Instr::i(op, r(rng), r(rng), imm(rng, IMM16_MAX)),
            },
            Fmt::J => Instr::j(op, r(rng), imm(rng, IMM19_MAX)),
            Fmt::N => Instr::n(op),
        };
        words.push(encode(&ins).expect("instrução gerada é válida"));
    }
    Program {
        segments: vec![Segment { base: 365, words, perm: Perm::Rx }],
        symbols: Default::default(),
        entry: 365,
        listing: vec![],
    }
}

/// Tarefa que carrega valores distintos em todos os registradores e
/// confere, `iters` vezes, que nenhum mudou; sai com 0 se tudo certo.
pub fn reg_keeper(base: i64, iters: i64) -> Program {
    let regs = [
        "a1", "a2", "a3", "a4", "a5", "a6", "a7", "a8", "a9", "t1", "t2", "s1", "s2", "s3", "s4", "s5", "s6", "s7",
        "s8", "s9", "s10", "s11", "lr", "sp",
    ];
    let mut src = String::new();
    for (i, r) in regs.iter().enumerate() {
        src += &format!("    la {r}, {}\n", base * (i as i64 + 1));
    }
    src += &format!("    la t3, {iters}\nlaco:\n");
    for (i, r) in regs.iter().enumerate() {
        src += &format!("    la t4, {}\n    bne {r}, t4, ruim\n", base * (i as i64 + 1));
    }
    src += "    addi t3, t3, -1\n    bp t3, laco\n    li a2, 0\n    li a1, SYS_EXIT\n    sys\nruim:\n    li a2, 1\n    li a1, SYS_EXIT\n    sys\n";
    prog(&src)
}

// ───────────────────────── execução verificada ─────────────────────────

/// O que uma execução verificada observou.
#[derive(Default, Clone, Debug)]
pub struct Observed {
    /// chamadas por número (índice NSYS = número inválido)
    pub syscalls: Vec<u64>,
    /// falhas por causa (índice = −causa)
    pub faults: Vec<u64>,
    /// endereços do texto do núcleo executados
    pub kernel_pcs: std::collections::BTreeSet<i64>,
    pub kernel_exits: u64,
    /// reinícios de tarefa pedidos (SYS_TASK com operação positiva)
    pub restarts: u64,
    /// maior número de passos seguidos em modo núcleo entre um trap e o
    /// retorno (o núcleo não é preemptível: isto limita a latência),
    /// separando os traps que reiniciam uma tarefa dos demais
    pub max_burst: u64,
    pub max_burst_restart: u64,
}

impl Observed {
    pub fn merge(&mut self, o: &Observed) {
        if self.syscalls.len() < o.syscalls.len() {
            self.syscalls.resize(o.syscalls.len(), 0);
        }
        for (a, b) in self.syscalls.iter_mut().zip(&o.syscalls) {
            *a += b;
        }
        if self.faults.len() < o.faults.len() {
            self.faults.resize(o.faults.len(), 0);
        }
        for (a, b) in self.faults.iter_mut().zip(&o.faults) {
            *a += b;
        }
        self.kernel_pcs.extend(&o.kernel_pcs);
        self.kernel_exits += o.kernel_exits;
        self.restarts += o.restarts;
        self.max_burst = self.max_burst.max(o.max_burst);
        self.max_burst_restart = self.max_burst_restart.max(o.max_burst_restart);
    }
}

/// Endereços do núcleo executados por QUALQUER execução verificada deste
/// processo de teste (união).
pub static KERNEL_COV: std::sync::Mutex<std::collections::BTreeSet<i64>> =
    std::sync::Mutex::new(std::collections::BTreeSet::new());

/// Maior profundidade de pilha do núcleo observada (palavras).
pub static KSTACK_MAX: std::sync::Mutex<i64> = std::sync::Mutex::new(0);

/// Maior trecho contínuo em modo núcleo observado: (sem reinício, com reinício).
pub static BURST_MAX: std::sync::Mutex<(u64, u64)> = std::sync::Mutex::new((0, 0));

/// Roda verificando os invariantes do núcleo a CADA saída do núcleo e a
/// memória a cada `mem_every` saídas e no fim. Para quando `done` diz sim.
pub fn run_checked(
    s: &mut Sys,
    max_steps: u64,
    mem_every: u64,
    done: impl FnMut(&Sys) -> bool,
) -> Result<Stop, String> {
    run_observed(s, max_steps, mem_every, done).map(|(stop, _)| stop)
}

/// Como `run_checked`, devolvendo também o que foi observado (cobertura
/// do núcleo, histograma de chamadas e falhas).
///
/// Além dos invariantes, confere por sonda da máquina que, depois do boot,
/// o núcleo NÃO acessa a área de usuário (quadros e tabelas de páginas) —
/// exceto enquanto executa um reinício de tarefa, que reconstrói um espaço
/// de endereços.
pub fn run_observed(
    s: &mut Sys,
    max_steps: u64,
    mem_every: u64,
    mut done: impl FnMut(&Sys) -> bool,
) -> Result<(Stop, Observed), String> {
    let mut kc = KCheck::new(&s.img, &s.m);
    let nsys = s.k("NSYS") as usize;
    let sys_task = s.k("SYS_TASK");
    let free_start = s.m.peek(s.k("boot_table") + s.k("BT_FREE")).unwrap().val();
    let mut obs = Observed { syscalls: vec![0; nsys + 1], faults: vec![0; 8], ..Default::default() };
    let mut stop = Stop::StepLimit;
    let mut booted = s.m.watch_from.is_some();
    // trecho em modo núcleo em curso: (passos, é um reinício?)
    let mut burst: Option<(u64, bool)> = None;
    for _ in 0..max_steps {
        if s.m.kernel {
            obs.kernel_pcs.insert(s.m.pc.val());
            if let Some(b) = burst.as_mut() {
                b.0 += 1;
            }
        }
        let (a1, a3) = (s.m.reg(1).val(), s.m.reg(3).val());
        match s.m.step() {
            Step::Exec => {}
            Step::Trap(c) => {
                burst = Some((0, false));
                if c == 0 {
                    let n = if (0..nsys as i64).contains(&a1) { a1 as usize } else { nsys };
                    obs.syscalls[n] += 1;
                    if a1 == sys_task && a3 > 0 {
                        obs.restarts += 1;
                        burst = Some((0, true));
                        s.m.watch_from = None; // o reinício reconstrói um espaço de endereços
                    }
                } else if c < 0 {
                    obs.faults[(-c) as usize] += 1;
                }
            }
            Step::Eret => {
                if let Some((n, restart)) = burst.take() {
                    if restart {
                        obs.max_burst_restart = obs.max_burst_restart.max(n);
                    } else {
                        obs.max_burst = obs.max_burst.max(n);
                    }
                }
                obs.kernel_exits += 1;
                booted = true;
                s.m.watch_from = Some(free_start - 364);
                kc.check_state(&s.img, &s.m, true)?;
                if obs.kernel_exits % mem_every == 0 {
                    kc.check_memory(&s.img, &s.m)?;
                }
                if done(s) {
                    break;
                }
            }
            Step::Halted(c) => {
                stop = Stop::Halted(c);
                break;
            }
            Step::Idle => {
                stop = Stop::Idle;
                break;
            }
            Step::DoubleFault => {
                stop = Stop::DoubleFault;
                break;
            }
        }
    }
    let _ = booted;
    KERNEL_COV.lock().unwrap().extend(&obs.kernel_pcs);
    {
        let mut b = BURST_MAX.lock().unwrap();
        b.0 = b.0.max(obs.max_burst);
        b.1 = b.1.max(obs.max_burst_restart);
    }
    kc.check_state(&s.img, &s.m, false)?;
    kc.check_memory(&s.img, &s.m)?;
    kc.measure_kstack(&s.m);
    {
        let mut d = KSTACK_MAX.lock().unwrap();
        *d = (*d).max(kc.max_kstack);
    }
    let panic = s.m.peek(s.img.ksym("panic_code")).unwrap().val();
    if panic != 0 {
        return Err(format!("PÂNICO do núcleo: código {panic}"));
    }
    if let Stop::Halted(c) = stop {
        if c < 0 {
            return Err(format!("núcleo desligou com código de pânico {c}"));
        }
    }
    if stop == Stop::DoubleFault {
        return Err("falha dupla".into());
    }
    if s.m.watch_hits != 0 {
        return Err(format!("o núcleo tocou memória de usuário {} vez(es) fora de boot/reinício", s.m.watch_hits));
    }
    Ok((stop, obs))
}

/// Roda sem verificações, só registrando a cobertura do núcleo (para os
/// caminhos de pânico, em que os invariantes não se aplicam).
pub fn run_cov(m: &mut Machine, max_steps: u64) -> Stop {
    let mut pcs = std::collections::BTreeSet::new();
    let mut stop = Stop::StepLimit;
    for _ in 0..max_steps {
        if m.kernel {
            pcs.insert(m.pc.val());
        }
        match m.step() {
            Step::Exec | Step::Trap(_) | Step::Eret => {}
            Step::Halted(c) => {
                stop = Stop::Halted(c);
                break;
            }
            Step::Idle => {
                stop = Stop::Idle;
                break;
            }
            Step::DoubleFault => {
                stop = Stop::DoubleFault;
                break;
            }
        }
    }
    KERNEL_COV.lock().unwrap().extend(&pcs);
    stop
}

// ───────────────────────── cenários-detectores ─────────────────────────
// Cenários de sistema que devolvem `Err` em vez de entrar em pânico e
// aceitam QUALQUER núcleo — usados para (a) validar o núcleo real e
// (b) testes de mutação (tests/mutants.rs): um núcleo com defeito
// plantado tem de fazer pelo menos um detector falhar.

pub type Detector = fn(&Program) -> Result<(), String>;

macro_rules! ensure {
    ($cond:expr, $($msg:tt)*) => {
        if !$cond {
            return Err(format!($($msg)*));
        }
    };
}

/// O par de vítimas padrão (cliente no endpoint 1 com console; servidor).
pub fn victims() -> Vec<TaskSpec> {
    vec![task("ping", user("ping")).send(0, 1, 7).con(), task("pong", user("pong")).recv(0, 1)]
}

fn victims_ok(s: &Sys) -> Result<(), String> {
    let dead = s.k("ST_DEAD");
    for name in ["ping", "pong"] {
        let t = s.task(name);
        ensure!((t.state, t.exit, t.fcause) == (dead, 0, 0), "{name} não concluiu: estado {} exit {} causa {}", t.state, t.exit, t.fcause);
    }
    ensure!(s.m.console_text().contains("ping: soma=65\n"), "saída {:?}", s.m.console_text());
    Ok(())
}

/// Confere que as vítimas concluíram corretamente (versão que entra em pânico).
pub fn assert_victims_ok(s: &Sys, ctx: &str) {
    victims_ok(s).unwrap_or_else(|e| panic!("{ctx}: {e}"));
}

pub fn sc_ping_pong(k: &Program) -> Result<(), String> {
    let mut s = boot_k(k, victims(), 0, Some(11))?;
    let stop = run_checked(&mut s, 1_000_000, 4, |_| false)?;
    ensure!(stop == Stop::Halted(0), "parada {stop:?}");
    ensure!(s.m.console_text() == "ping: soma=65\n", "saída {:?}", s.m.console_text());
    victims_ok(&s)?;
    // De novo, com o contador de carimbos prestes a dar a volta: o carimbo
    // zero tem de ser pulado (zero significa "sem recibo").
    let mut s = boot_k(k, victims(), 0, None)?;
    while s.m.step() != Step::Eret {}
    let seq = s.k("seq");
    s.m.poke(seq, Word::lit(-3));
    let stop = run_checked(&mut s, 1_000_000, 4, |_| false).map_err(|e| format!("volta dos carimbos: {e}"))?;
    ensure!(stop == Stop::Halted(0), "volta dos carimbos: parada {stop:?}");
    ensure!(s.m.peek(seq).unwrap().val() > 0, "o contador de carimbos não passou pelo zero");
    victims_ok(&s).map_err(|e| format!("volta dos carimbos: {e}"))
}

pub fn sc_preempt(k: &Program) -> Result<(), String> {
    // quem não cede vem PRIMEIRO no rodízio; as vítimas dependem da preempção
    let mut tasks = vec![task("spin", user("spin")), task("k", reg_keeper(1_000_003, 400))];
    tasks.extend(victims());
    let mut s = boot_k(k, tasks, 300, Some(12))?;
    let dead = s.k("ST_DEAD");
    run_checked(&mut s, 3_000_000, 512, |s| {
        s.task("ping").state == dead && s.task("pong").state == dead && s.task("k").state == dead
    })?;
    ensure!((s.task("k").state, s.task("k").exit) == (dead, 0), "guardião de registradores: estado {} exit {}", s.task("k").state, s.task("k").exit);
    ensure!(s.task("spin").reg(-1) > 100, "spin não rodou");
    victims_ok(&s)
}

pub const FAULT_CASES: [(&str, &str, i64); 14] = {
    use tenerura_t27::isa::cause::*;
    [
        ("leitura pelo ponteiro nulo", "ld a1, 0(zr)", LOAD_FAULT),
        ("leitura logo abaixo de zero", "ld a1, -3(zr)", LOAD_FAULT),
        ("escrita no próprio código", "la a1, USER_TEXT\n st a1, 0(a1)", STORE_FAULT),
        ("execução da pilha", "la a1, -400\n jr a1", FETCH_FAULT),
        ("salto para o vazio", "la a1, 100000000\n jr a1", FETCH_FAULT),
        ("endereço não canônico", "la a1, 200000000\n ld a1, 0(a1)", LOAD_FAULT),
        ("endereço físico do núcleo", "ld a1, 5(zr)", LOAD_FAULT),
        ("endereço do dispositivo", "la a1, 14348907\n st a1, 0(a1)", STORE_FAULT),
        ("troca da tabela de páginas", "csrw ptroot, zr", PRIVILEGE),
        ("retorno de trap forjado", "eret", PRIVILEGE),
        ("leitura de registrador de controle", "csrr a1, scratch", PRIVILEGE),
        ("instrução ilegal", ".word 0", ILLEGAL),
        ("breakpoint", "brk", BREAKPOINT),
        ("estouro de pilha", "1: push a1\n j 1b", STORE_FAULT),
    ]
};

pub fn sc_faults(k: &Program) -> Result<(), String> {
    for (what, body, want) in FAULT_CASES {
        let mut tasks = vec![task("reu", prog(body))];
        tasks.extend(victims());
        let mut s = boot_k(k, tasks, 0, None)?;
        let stop = run_checked(&mut s, 1_000_000, 8, |_| false)?;
        ensure!(stop == Stop::Halted(0), "[{what}] parada {stop:?}");
        let t = s.task("reu");
        ensure!((t.state, t.fcause) == (s.k("ST_DEAD"), want), "[{what}] estado {} causa {}", t.state, t.fcause);
        victims_ok(&s).map_err(|e| format!("[{what}] {e}"))?;
    }
    Ok(())
}

const CALL: &str = "li a1, SYS_CALL\n li a2, 0\n li a3, 5\n li t1, FOREVER\n sys\n mov s1, a1\n mov s2, a2\n li a2, 0\n j exit";
const RECV: &str = "li a1, SYS_RECV\n li a2, 0\n li t1, FOREVER\n sys\n";

pub fn sc_dead_peer(k: &Program) -> Result<(), String> {
    // o servidor recebe a chamada e morre (falha ou exit) sem responder
    for server_end in ["st zr, 0(zr)", "li a2, 9\n j exit"] {
        let server = prog(&format!("{RECV} {server_end}"));
        let mut s = boot_k(k, vec![task("cliente", prog(CALL)).send(0, 1, 0), task("servidor", server).recv(0, 1)], 0, None)?;
        let stop = run_checked(&mut s, 300_000, 1, |_| false)?;
        ensure!(stop == Stop::Halted(0), "parada {stop:?}");
        let c = s.task("cliente");
        ensure!((c.reg(-1), c.reg(-2)) == (-1, s.k("E_DEADPEER")), "call devolveu ({}, {})", c.reg(-1), c.reg(-2));
        ensure!(c.fcause == 0, "o cliente não sobreviveu");
    }
    Ok(())
}

pub fn sc_invalid_requests(k: &Program) -> Result<(), String> {
    // Cada pedido inválido devolve erro e a tarefa segue viva. Registro:
    // a1·100 + a2 (a2 = código de erro quando a1 = −1).
    let body = "
        .macro REC r
            li t2, 100
            mul \\r, a1, t2
            add \\r, \\r, a2
        .endm
        li t1, 0
        li a1, SYS_SEND\n li a2, 1\n sys\n REC s1             ; envio em capacidade de recepção
        li a1, SYS_RECV\n li a2, 0\n sys\n REC s2             ; recepção em capacidade de envio
        li a1, SYS_SEND\n li a2, -1\n sys\n REC s3            ; slot negativo
        li a1, SYS_SEND\n li a2, 9\n sys\n REC s4             ; slot além do fim
        li a1, SYS_SEND\n la a2, 3812798742493\n sys\n REC s5 ; slot = maior palavra
        li a1, SYS_SEND\n li a2, 2\n sys\n REC s6             ; slot vazio
        li a1, 99\n sys\n REC s7                              ; chamada inexistente
        li a1, -1\n sys\n REC s8                              ; número negativo
        li a1, SYS_REPLY\n li a2, 0\n sys\n REC s9            ; recibo zero
        li a1, SYS_PUTC\n li a2, 8\n li a3, 'x'\n sys\n REC s10   ; console sem capacidade
        li a1, SYS_RECV\n li a2, 1\n sys\n REC s11            ; paciência 0, nada pendente: 0 (a2 intacto)
        li a1, NSYS\n sys\n REC t3                            ; primeiro número além da tabela
        li a1, SYS_REPLY\n li a2, 77\n sys\n REC t4           ; recibo inventado
        li a1, SYS_TASK\n li a2, 0\n li a3, 1\n sys\n REC a9  ; reinício com capacidade de outro tipo
        li a1, SYS_IRQ_ACK\n li a2, 1\n sys\n REC a8          ; reconhecimento com capacidade de outro tipo
        li a1, SYS_SEND\n li a2, 0\n li t1, 0\n sys\n REC a7  ; paciência 0, ninguém recebe: 0
        li a2, 0
        li a1, SYS_EXIT
        sys
    ";
    let mut s = boot_k(k, vec![task("t", prog(body)).send(0, 1, 0).recv(1, 1)], 0, None)?;
    let stop = run_checked(&mut s, 300_000, 1, |_| false)?;
    ensure!(stop == Stop::Halted(0), "parada {stop:?}");
    let t = s.task("t");
    let (badcap, nosys, nocaller) = (-100 + s.k("E_BADCAP"), -100 + s.k("E_NOSYS"), -100 + s.k("E_NOCALLER"));
    let got: Vec<i64> = (1..=11).map(|i| t.reg(-i)).chain([t.reg(12), t.reg(13), t.reg(9), t.reg(8), t.reg(7)]).collect();
    let want = vec![
        badcap, badcap, badcap, badcap, badcap, badcap, nosys, nosys, nocaller, badcap, 1, nosys, nocaller, badcap,
        badcap, 0,
    ];
    ensure!(got == want, "resultados {got:?}, esperado {want:?}");
    ensure!((t.exit, t.fcause) == (0, 0), "a tarefa não sobreviveu");
    ensure!(s.m.console_text().is_empty(), "console sem capacidade imprimiu {:?}", s.m.console_text());
    Ok(())
}

pub fn sc_fifo_badges(k: &Program) -> Result<(), String> {
    // três emissores bloqueiam em ordem de chegada 33, 22, 11 (o de menor
    // sleep chega primeiro); o receptor só começa a receber depois. Um
    // quarto envia quando o receptor já espera (entrega direta).
    let sender = |delay: i64, v: i64| {
        prog(&format!(
            "li a1, SYS_SLEEP\n li a2, {delay}\n sys
             li a1, SYS_SEND\n li a2, 0\n li t1, FOREVER
             la a3, {v}\n neg a4, a3\n li a5, 5\n li a6, 6\n li a7, 7\n li a8, 8\n la a9, 3812798742493
             sys
             mov a2, a1
             j exit"
        ))
    };
    let receiver = prog(
        "li a1, SYS_SLEEP\n la a2, 60000\n sys
         li s9, 0
         li a1, SYS_RECV\n li a2, 0\n li t1, FOREVER\n sys\n mov s1, a2\n mov s4, a3\n add s9, s9, t1
         li a1, SYS_RECV\n li a2, 0\n li t1, FOREVER\n sys\n mov s2, a2\n mov s5, a3\n add s9, s9, t1
         li a1, SYS_RECV\n li a2, 0\n li t1, FOREVER\n sys\n mov s3, a2\n mov s6, a3\n add s9, s9, t1
         add s7, a4, a3
         mov s8, a9
         add s9, s9, a5
         add s9, s9, a6
         add s9, s9, a7
         add s9, s9, a8
         li a1, SYS_RECV\n li a2, 0\n li t1, FOREVER\n sys\n mov s10, a2\n mov s11, a3\n add s9, s9, t1
         li a2, 0
         j exit",
    );
    let late = prog("li a1, SYS_SLEEP\n la a2, 120000\n sys\n li a1, SYS_SEND\n li a2, 0\n li a3, 1004\n li t1, FOREVER\n sys\n mov a2, a1\n j exit");
    let mut s = boot_k(
        k,
        vec![
            task("e1", sender(30000, 1001)).send(0, 4, 11),
            task("e2", sender(20000, 1002)).send(0, 4, 22),
            task("e3", sender(10000, 1003)).send(0, 4, 33),
            task("e4", late).send(0, 4, 44),
            task("r", receiver).recv(0, 4),
        ],
        0,
        None,
    )?;
    let stop = run_checked(&mut s, 2_000_000, 1, |_| false)?;
    ensure!(stop == Stop::Halted(0), "parada {stop:?}");
    let r = s.task("r");
    let got: Vec<i64> = (1..=11).map(|i| r.reg(-i)).collect();
    // s9 = 5+6+7+8 + soma dos recibos (0: nenhum send simples traz recibo)
    let want = vec![33, 22, 11, 1003, 1002, 1001, 0, 3812798742493, 26, 44, 1004];
    ensure!(got == want, "receptor viu {got:?}, esperado {want:?}");
    for e in ["e1", "e2", "e3", "e4"] {
        ensure!(s.task(e).exit == 1, "{e}: send devolveu {}", s.task(e).exit);
    }
    Ok(())
}

pub fn sc_cap_forgery(k: &Program) -> Result<(), String> {
    // O atacante põe (tipo, objeto, selo) de uma capacidade de envio para o
    // endpoint da vítima em TODOS os registradores e tenta todos os slots
    // fora de 0..8 — inclusive os que, sem checagem de limite, cairiam
    // sobre os registradores salvos no próprio TCB.
    let mut body = String::new();
    for slot in (-30..0).chain(9..40) {
        body += "    li s1, 1\n li s2, 1\n li s3, 1\n li s4, 1\n li s5, 1\n li s6, 1\n li s7, 1\n li s8, 1\n";
        body += "    li s9, 1\n li s10, 1\n li s11, 1\n li lr, 1\n li sp, 1\n li t2, 1\n li t3, 1\n li t4, 1\n";
        body += &format!("    li a1, SYS_SEND\n li a2, {slot}\n li t1, 0\n li a3, 0\n li a4, 1\n li a5, 1\n li a6, 1\n li a7, 1\n li a8, 1\n li a9, 1\n sys\n");
        body += "    li t2, -1\n bne a1, t2, furou\n";
    }
    // Confusão de tipos: uma capacidade de ENVIO cujo objeto coincide com o
    // índice da vítima (0) não vale como capacidade de tarefa; uma cujo
    // objeto é 2 não vale como capacidade da linha de interrupção 2.
    for (slot, call, op) in [(0, "SYS_TASK", -1), (0, "SYS_TASK", 1), (1, "SYS_IRQ_ACK", 0)] {
        body += &format!("    li a1, {call}\n li a2, {slot}\n li a3, {op}\n sys\n li t2, -1\n bne a1, t2, furou\n");
    }
    body += "    li a2, 0\n li a1, SYS_EXIT\n sys\nfurou:\n li a2, 1\n li a1, SYS_EXIT\n sys\n";
    // a vítima espera no endpoint 1
    let victim = prog(&format!("{RECV} mov s1, a3\n li a2, 7\n j exit"));
    let mut s = boot_k(
        k,
        vec![task("vitima", victim).recv(0, 1), task("atacante", prog(&body)).send(0, 5, 0).send(1, 2, 0)],
        0,
        None,
    )?;
    let stop = run_checked(&mut s, 2_000_000, 1, |_| false)?;
    ensure!(stop == Stop::Halted(3), "parada {stop:?}");
    ensure!(s.task("atacante").exit == 0 && s.task("atacante").fcause == 0, "alguma capacidade forjada foi aceita");
    ensure!(s.task("vitima").state == s.k("ST_BLK_RECV"), "a vítima recebeu uma mensagem forjada");
    Ok(())
}

pub fn sc_sleep(k: &Program) -> Result<(), String> {
    let sleeper = |ticks: i64, ch: char| {
        prog(&format!(
            "li a1, SYS_TIME\n sys\n mov s1, a2
             li a1, SYS_SLEEP\n li a2, {ticks}\n sys
             li a1, SYS_TIME\n sys\n sub s2, a2, s1
             li a2, '{ch}'\n call putc
             li t1, {ticks}
             cmp a2, s2, t1
             j exit"
        ))
    };
    let mut s = boot_k(
        k,
        vec![task("a", sleeper(30000, 'a')).con(), task("b", sleeper(10000, 'b')).con(), task("c", sleeper(20000, 'c')).con()],
        0,
        None,
    )?;
    let stop = run_checked(&mut s, 2_000_000, 1, |_| false)?;
    ensure!(stop == Stop::Halted(0), "parada {stop:?}");
    ensure!(s.m.console_text() == "bca", "ordem de despertar {:?}", s.m.console_text());
    for n in ["a", "b", "c"] {
        ensure!(s.task(n).exit >= 0, "{n} dormiu menos que o pedido");
    }
    // sleep gigante e paciência gigante: limitados a SLEEP_MAX (o prazo
    // fica a menos de meia volta do relógio). Sistema ocioso ⇒ o WFI salta.
    for (what, body) in [
        ("sleep gigante", "li a1, SYS_SLEEP\n la a2, 3812798742493\n sys\n li a2, 0\n j exit"),
        ("paciência gigante", "li a1, SYS_RECV\n li a2, 0\n la t1, 3812798742493\n sys\n mov a2, a1\n j exit"),
    ] {
        let mut s = boot_k(k, vec![task("t", prog(body)).recv(0, 1)], 0, None)?;
        let stop = run_checked(&mut s, 300_000, 1, |_| false)?;
        ensure!(stop == Stop::Halted(0), "{what}: parada {stop:?}");
        let max = s.k("SLEEP_MAX");
        ensure!(s.m.time >= max && s.m.time < max + 100_000, "{what}: tempo final {}", s.m.time);
        ensure!(s.task("t").exit == 0, "{what}: resultado {}", s.task("t").exit);
    }
    Ok(())
}

pub fn sc_clock_wrap(k: &Program) -> Result<(), String> {
    // cada sleep confere o tempo decorrido (aritmética modular); o relógio
    // dá a volta no meio da execução
    let sleeper = prog(
        "    li a1, SYS_TIME\n sys\n mov s2, a2
             li a1, SYS_SLEEP\n la a2, 20000\n sys          ; este sleep contém a volta do relógio
             li a1, SYS_TIME\n sys
             sub s3, a2, s2
             la t1, 20000
             blt s3, t1, cedo
             la t1, 60000
             bgt s3, t1, tarde
             bnn a2, tarde                 ; depois da volta o relógio é negativo
             li s1, 30
         1:  li a1, SYS_TIME\n sys\n mov s2, a2
             li a1, SYS_SLEEP\n li a2, 700\n sys
             li a1, SYS_TIME\n sys
             sub s3, a2, s2
             li t1, 700
             blt s3, t1, cedo
             la t1, 40000
             bgt s3, t1, tarde
             addi s1, s1, -1
             bp s1, 1b
             li a2, 'z'\n call putc
             li a2, 0\n j exit
         cedo:  li a2, 1\n j exit
         tarde: li a2, 2\n j exit",
    );
    let mut tasks = victims();
    tasks.push(task("spin", user("spin")));
    tasks.push(task("sleeper", sleeper).con());
    let mut s = boot_k(k, tasks, 0, None)?;
    // termina o boot e SÓ ENTÃO aproxima o relógio da volta, para que ela
    // caia no meio dos sleeps (e não durante o boot)
    while s.m.step() != Step::Eret {}
    s.m.time = tenerura_t27::word::M27 - 9_000;
    let dead = s.k("ST_DEAD");
    run_checked(&mut s, 3_000_000, 64, |s| s.task("sleeper").state == dead)?;
    ensure!(Word::wrap128(s.m.time as i128).val() < 0, "o relógio não deu a volta");
    let t = s.task("sleeper");
    ensure!((t.state, t.exit, t.fcause) == (dead, 0, 0), "sleeper: estado {} exit {} (1 = acordou cedo, 2 = tarde)", t.state, t.exit);
    let out = s.m.console_text();
    ensure!(out.contains("ping: soma=65\n") && out.contains('z'), "saída {out:?}");
    ensure!(s.task("spin").reg(-1) > 1000, "spin parou");
    Ok(())
}

pub fn sc_timeouts(k: &Program) -> Result<(), String> {
    // Um servidor que recebe e NUNCA responde. O cliente chama com prazo:
    // tem de voltar com 0 depois de ≥ prazo passos — sem depender de o
    // servidor colaborar. A resposta tardia do servidor é recusada.
    let server = prog(&format!(
        "{RECV} mov s1, t1
         li a1, SYS_SLEEP\n la a2, 50000\n sys
         li a1, SYS_REPLY\n mov a2, s1\n sys            ; tarde demais: recibo vencido
         mov s2, a1\n mov s3, a2
         li a2, 0\n j exit"
    ));
    let client = prog(
        "li a1, SYS_TIME\n sys\n mov s4, a2
         li a1, SYS_CALL\n li a2, 0\n li a3, 5\n la t1, 8000\n sys
         mov s1, a1
         li a1, SYS_TIME\n sys\n sub s2, a2, s4           ; quanto esperou
         li a1, SYS_RECV\n li a2, 1\n la t1, 3000\n sys   ; recv com prazo em endpoint mudo
         mov s3, a1
         li a1, SYS_SEND\n li a2, 2\n la t1, 3000\n sys   ; send com prazo sem receptor
         mov s5, a1
         ; paciência ZERO não espera nem cede o processador: 50 tentativas de
         ; envio e 50 de recepção cabem com folga numa fatia
         li a1, SYS_TIME\n sys\n mov s6, a2
         li s7, 50
     1:  li a1, SYS_SEND\n li a2, 2\n li t1, 0\n sys
         bnz a1, 2f
         li a1, SYS_RECV\n li a2, 1\n li t1, 0\n sys
         bnz a1, 2f
         addi s7, s7, -1
         bp s7, 1b
     2:  li a1, SYS_TIME\n sys\n sub s6, a2, s6
         li a2, 0\n j exit",
    );
    // uma tarefa sempre pronta ao lado: se uma tentativa sem espera cedesse
    // o processador, cada uma custaria uma fatia inteira
    let mut s = boot_k(
        k,
        vec![
            task("cliente", client).send(0, 1, 0).recv(1, 2).send(2, 3, 0),
            task("servidor", server).recv(0, 1),
            task("spin", user("spin")),
        ],
        0,
        None,
    )?;
    let dead = s.k("ST_DEAD");
    run_checked(&mut s, 2_000_000, 1, |s| s.task("cliente").state == dead && s.task("servidor").state == dead)?;
    let c = s.task("cliente");
    ensure!(c.reg(-1) == 0, "call com prazo devolveu {} (esperado 0)", c.reg(-1));
    ensure!(c.reg(-2) >= 8000 && c.reg(-2) < 8000 + 15000, "call esperou {} passos (prazo 8000)", c.reg(-2));
    ensure!((c.reg(-3), c.reg(-5)) == (0, 0), "recv/send com prazo devolveram ({}, {})", c.reg(-3), c.reg(-5));
    ensure!(c.reg(-7) == 0, "alguma tentativa sem espera devolveu ≠ 0");
    // Limite de 80 000: medido 34 503 no núcleo real (≈15 mil passos das 100
    // chamadas + 3–4 preempções de 5000, porque `spin` está sempre pronta) e
    // ≈280 mil nos mutantes em que a tentativa cede o processador. O limite
    // separa os dois regimes; não é uma meta de desempenho.
    ensure!(c.reg(-6) < 80_000, "100 tentativas sem espera levaram {} passos (cederam o processador?)", c.reg(-6));
    ensure!((c.exit, c.fcause) == (0, 0), "cliente não concluiu");
    let sv = s.task("servidor");
    ensure!((sv.reg(-2), sv.reg(-3)) == (-1, s.k("E_NOCALLER")), "resposta tardia devolveu ({}, {})", sv.reg(-2), sv.reg(-3));
    Ok(())
}

pub fn sc_deferred_reply(k: &Program) -> Result<(), String> {
    // O servidor recebe TRÊS chamadas sem responder nenhuma (guarda os
    // recibos) e depois responde na ordem inversa; cada cliente recebe a
    // resposta do SEU pedido. Um recibo usado duas vezes é recusado.
    let server = prog(&format!(
        "{RECV} mov s1, t1\n mov s4, a3
         {RECV} mov s2, t1\n mov s5, a3
         {RECV} mov s3, t1\n mov s6, a3
         li a1, SYS_REPLY\n mov a2, s3\n addi a3, s6, 1000\n sys
         li a1, SYS_REPLY\n mov a2, s2\n addi a3, s5, 1000\n sys
         li a1, SYS_REPLY\n mov a2, s1\n addi a3, s4, 1000\n sys
         li a1, SYS_REPLY\n mov a2, s1\n li a3, 666\n sys   ; recibo já usado
         mov s7, a1\n mov s8, a2
         li a2, 0\n j exit"
    ));
    let client = |v: i64| {
        prog(&format!(
            "li a1, SYS_CALL\n li a2, 0\n li a3, {v}\n li t1, FOREVER\n sys
             bnp a1, 1f
             mov a2, a3\n j exit
         1:  li a2, -1\n j exit"
        ))
    };
    let mut s = boot_k(
        k,
        vec![
            task("c1", client(1)).send(0, 1, 0),
            task("c2", client(2)).send(0, 1, 0),
            task("c3", client(3)).send(0, 1, 0),
            task("servidor", server).recv(0, 1),
        ],
        0,
        None,
    )?;
    let stop = run_checked(&mut s, 1_000_000, 1, |_| false)?;
    ensure!(stop == Stop::Halted(0), "parada {stop:?}");
    for (n, want) in [("c1", 1001), ("c2", 1002), ("c3", 1003)] {
        ensure!(s.task(n).exit == want, "{n} recebeu {} (esperado {want})", s.task(n).exit);
    }
    let sv = s.task("servidor");
    ensure!((sv.reg(-7), sv.reg(-8)) == (-1, s.k("E_NOCALLER")), "recibo reusado devolveu ({}, {})", sv.reg(-7), sv.reg(-8));
    Ok(())
}

/// Sistema da demonstração 2: driver de console, supervisor, serviço
/// instável, cliente resiliente e eco.
pub fn demo2_tasks(requests: i64) -> Vec<TaskSpec> {
    vec![
        task("condrv", user("condrv"))
            .recv(0, 10)
            .cap(1, CAP_IRQ, 2, 0)
            .device(DEV_CONSOLE, 100)
            .irq(2, 10, -1),
        task("super", user("super"))
            .recv(0, 20)
            .cap(1, CAP_TASK, 2, 0)
            .cap(2, CAP_TASK, 3, 0)
            .cap(3, CAP_TASK, 4, 0)
            .send(7, 10, 0),
        task("flaky", user("flaky")).recv(0, 30).fault(20, -1),
        task("client", with_word(user("client"), "count", Word::lit(requests))).send(0, 30, 0).send(7, 10, 0).fault(20, -2),
        task("echo", user("echo")).send(7, 10, 0).fault(20, -3),
    ]
}

pub fn sc_supervision(k: &Program) -> Result<(), String> {
    // O serviço falha a cada 4 pedidos. Com 10 pedidos: falha nos pedidos
    // 4, 7 e 10 (cada encarnação atende 3 e morre no 4º) → 3 reinícios, o
    // máximo que a política do supervisor admite; o cliente conclui os 10.
    let mut s = boot_k(k, demo2_tasks(10), 0, Some(21))?;
    // 1ª parte: até o sistema ficar ocioso (o eco espera entrada; só uma
    // interrupção do console pode acordar a máquina)
    let (stop, obs) = run_observed(&mut s, 8_000_000, 16, |_| false)?;
    let out = s.m.console_text();
    ensure!(stop == Stop::Idle, "parada {stop:?}; saída {out:?}");
    let want = "[sup] tarefa 2 falhou, causa -5: reinicio 1\n\
                [sup] tarefa 2 falhou, causa -5: reinicio 2\n\
                [sup] tarefa 2 falhou, causa -5: reinicio 3\n\
                cliente: pedidos concluidos 10, repeticoes 3\n\
                [sup] tarefa 3 saiu com codigo 0\n";
    ensure!(out == want, "saída {out:?}");
    ensure!(obs.restarts == 3, "reinícios pedidos: {}", obs.restarts);
    // 2ª parte: alguém digita
    s.type_text("oi q");
    let (stop, _) = run_observed(&mut s, 2_000_000, 16, |_| false)?;
    let out = s.m.console_text();
    ensure!(stop == Stop::Idle, "parada {stop:?}; saída {out:?}");
    ensure!(out == format!("{want}OI [sup] tarefa 4 saiu com codigo 0\n"), "saída {out:?}");
    let f = s.task("flaky");
    ensure!(f.gen == 3, "geração do serviço: {}", f.gen);
    ensure!(f.state == s.k("ST_BLK_RECV"), "serviço deveria estar de pé esperando pedidos (estado {})", f.state);
    for n in ["client", "echo"] {
        let t = s.task(n);
        ensure!((t.state, t.exit, t.fcause) == (s.k("ST_DEAD"), 0, 0), "{n}: estado {} exit {} causa {}", t.state, t.exit, t.fcause);
    }
    ensure!(s.task("condrv").state == s.k("ST_BLK_RECV"), "driver deveria estar esperando");
    Ok(())
}

pub fn sc_restart_is_fresh(k: &Program) -> Result<(), String> {
    // Uma tarefa suja TUDO o que é seu (dados, pilha, registradores) e
    // falha. O supervisor a reinicia. A nova encarnação confere que está
    // no estado inicial e sai com 0; senão sai com o número do que achou sujo.
    let dirty = prog(
        "    bnz s1, sujo1                 ; registradores zerados?
             bnz a9, sujo1
             bnz lr, sujo1
             la t2, marca
             ld t3, 0(t2)
             li t4, 7
             bne t3, t4, sujo2             ; dados restaurados ao valor inicial?
             ld t3, 1(t2)
             bnz t3, sujo2                 ; e a parte zerada também?
             ld t3, -5(sp)
             bnz t3, sujo3                 ; pilha zerada?
             li a1, SYS_TASK\n li a2, 0\n li a3, 0\n sys   ; consulta a própria geração
             bp a5, 9f                     ; geração ≥ 1: é a nova encarnação — tudo certo
             ; primeira vida: suja tudo e falha
             li t3, 99\n la t2, marca\n st t3, 0(t2)\n st t3, 1(t2)
             st t3, -5(sp)
             li s1, 5\n li a9, 5\n li lr, 5
             st zr, 0(zr)
         9:  li a2, 0\n j exit
         sujo1: li a2, 1\n j exit
         sujo2: li a2, 2\n j exit
         sujo3: li a2, 3\n j exit
             .data
         marca: .word 7, 0",
    );
    let sup = prog(&format!(
        "{RECV} mov s1, a4                 ; causa do 1º aviso (falha)
         li a1, SYS_TASK\n li a2, 1\n li a3, 1\n sys
         mov s2, a1
         {RECV} mov s3, a4\n mov s4, a7    ; 2º aviso: saída voluntária, código
         li a2, 0\n j exit"
    ));
    let mut s = boot_k(
        k,
        vec![
            task("sup", sup).recv(0, 20).cap(1, CAP_TASK, 1, 0),
            task("suja", dirty).fault(20, -1).cap(0, CAP_TASK, 1, 0),
        ],
        0,
        Some(31),
    )?;
    let stop = run_checked(&mut s, 2_000_000, 1, |_| false)?;
    ensure!(stop == Stop::Halted(0), "parada {stop:?}");
    let sp = s.task("sup");
    ensure!(sp.reg(-1) == tenerura_t27::isa::cause::STORE_FAULT, "1º aviso: causa {}", sp.reg(-1));
    ensure!(sp.reg(-2) == 1, "reinício devolveu {}", sp.reg(-2));
    ensure!((sp.reg(-3), sp.reg(-4)) == (0, 0), "2º aviso: causa {} código {} (≠ 0 = algo ficou sujo)", sp.reg(-3), sp.reg(-4));
    ensure!(s.task("suja").gen == 1, "geração {}", s.task("suja").gen);
    Ok(())
}

pub fn sc_kill_and_restart_wake_dependents(k: &Program) -> Result<(), String> {
    // Um cliente espera resposta de um servidor que nunca responde. O
    // supervisor (a) reinicia o servidor: o cliente acorda com E_DEADPEER;
    // o cliente chama de novo; (b) o supervisor MATA o servidor: idem.
    let server = prog(&format!("1: {RECV} j 1b"));
    let client = prog(
        "li a1, SYS_CALL\n li a2, 0\n li t1, FOREVER\n sys\n mov s1, a1\n mov s2, a2
         li a1, SYS_CALL\n li a2, 0\n li t1, FOREVER\n sys\n mov s3, a1\n mov s4, a2
         li a2, 0\n j exit",
    );
    let sup = prog(
        "li a1, SYS_SLEEP\n la a2, 20000\n sys
         li a1, SYS_TASK\n li a2, 1\n li a3, 1\n sys\n mov s1, a1          ; reinicia
         li a1, SYS_SLEEP\n la a2, 20000\n sys
         li a1, SYS_TASK\n li a2, 1\n li a3, -1\n sys\n mov s2, a1         ; mata
         li a1, SYS_TASK\n li a2, 1\n li a3, -1\n sys\n mov s3, a1         ; já morta: 0
         li a1, SYS_TASK\n li a2, 1\n li a3, 0\n sys\n mov s4, a2\n mov s5, a3\n mov s6, a5
         li a2, 0\n j exit",
    );
    let mut s = boot_k(
        k,
        vec![
            task("servidor", server).recv(0, 1),
            task("cliente", client).send(0, 1, 0),
            task("sup", sup).cap(1, CAP_TASK, 0, 0),
        ],
        0,
        None,
    )?;
    let stop = run_checked(&mut s, 2_000_000, 1, |_| false)?;
    ensure!(stop == Stop::Halted(0), "parada {stop:?}");
    let c = s.task("cliente");
    let dp = s.k("E_DEADPEER");
    ensure!((c.reg(-1), c.reg(-2), c.reg(-3), c.reg(-4)) == (-1, dp, -1, dp), "cliente viu ({}, {}) e ({}, {})", c.reg(-1), c.reg(-2), c.reg(-3), c.reg(-4));
    let sp = s.task("sup");
    ensure!((sp.reg(-1), sp.reg(-2), sp.reg(-3)) == (1, 1, 0), "reinício/morte/morte devolveram ({}, {}, {})", sp.reg(-1), sp.reg(-2), sp.reg(-3));
    ensure!((sp.reg(-4), sp.reg(-5), sp.reg(-6)) == (s.k("ST_DEAD"), s.k("CAUSE_KILLED"), 1), "consulta devolveu estado {} causa {} geração {}", sp.reg(-4), sp.reg(-5), sp.reg(-6));
    Ok(())
}

pub fn sc_driver_irq(k: &Program) -> Result<(), String> {
    // Entrada em duas levas: o eco lê pelo driver; a 2ª leva chega com o
    // sistema inteiro ocioso (só uma interrupção pode acordá-lo). Um
    // segundo leitor simultâneo é recusado pelo driver (−1).
    let echo2 = prog_drv(
        "    li a1, SYS_SLEEP\n la a2, 30000\n sys
             call getc                     ; já há um leitor esperando (echo): −1
             mov s1, a2
             li a2, 0\n j exit",
    );
    let mut s = boot_k(
        k,
        vec![
            task("condrv", user("condrv")).recv(0, 10).cap(1, CAP_IRQ, 2, 0).device(DEV_CONSOLE, 100).irq(2, 10, -1),
            task("echo", user("echo")).send(7, 10, 0),
            task("echo2", echo2).send(7, 10, 0),
        ],
        0,
        Some(41),
    )?;
    let (stop, _) = run_observed(&mut s, 2_000_000, 1, |_| false)?;
    ensure!(stop == Stop::Idle, "sem entrada o sistema deveria ficar ocioso esperando (parada {stop:?})");
    ensure!(s.task("echo2").reg(-1) == -1, "segundo leitor recebeu {}", s.task("echo2").reg(-1));
    s.type_text("abc");
    let (stop, _) = run_observed(&mut s, 2_000_000, 1, |_| false)?;
    ensure!(stop == Stop::Idle, "parada {stop:?}");
    ensure!(s.m.console_text() == "ABC", "saída {:?}", s.m.console_text());
    s.type_text("x1q");
    let (stop, _) = run_observed(&mut s, 2_000_000, 1, |_| false)?;
    ensure!(stop == Stop::Idle, "parada {stop:?}");
    ensure!(s.m.console_text() == "ABCX1", "saída {:?}", s.m.console_text());
    ensure!((s.task("echo").state, s.task("echo").exit) == (s.k("ST_DEAD"), 0), "eco não encerrou");
    // Até aqui a entrada chegou com a máquina ociosa (o núcleo viu a linha
    // ao sair do WFI). Agora com uma tarefa RODANDO: a interrupção de
    // dispositivo chega como trap em modo usuário.
    let mut s = boot_k(
        k,
        vec![
            task("spin", user("spin")),
            task("condrv", user("condrv")).recv(0, 10).cap(1, CAP_IRQ, 2, 0).device(DEV_CONSOLE, 100).irq(2, 10, -1),
            task("echo", user("echo")).send(7, 10, 0),
        ],
        0,
        None,
    )?;
    let dead = s.k("ST_DEAD");
    run_checked(&mut s, 200_000, 16, |_| false)?;
    // um caractere a cada poucos passos: várias interrupções, algumas com o
    // driver ocupado (ficam PENDENTES até o próximo recv dele)
    for (i, ch) in "ternario q".chars().enumerate() {
        s.type_text(&ch.to_string());
        run_checked(&mut s, 150 + 977 * (i as u64 % 3), 16, |_| false)?;
    }
    run_checked(&mut s, 3_000_000, 16, |s| s.task("echo").state == dead)?;
    ensure!(s.m.console_text() == "TERNARIO ", "saída {:?}", s.m.console_text());
    ensure!(s.m.stats.traps_irq_dev >= 1, "nenhuma interrupção de dispositivo chegou como trap");
    ensure!(s.task("spin").state == s.k("ST_READY"), "a tarefa interrompida não seguiu");
    // Terceiro caminho: a entrada chega com o núcleo ocioso esperando um
    // PRAZO (há quem durma). A interrupção é atendida e o dorminhoco ainda
    // acorda na hora certa.
    let sleeper = prog(
        "li a1, SYS_TIME\n sys\n mov s1, a2
         li a1, SYS_SLEEP\n la a2, 300000\n sys
         li a1, SYS_TIME\n sys\n sub s2, a2, s1
         la t2, 300000
         cmp a2, s2, t2
         j exit",
    );
    let mut s = boot_k(
        k,
        vec![
            task("condrv", user("condrv")).recv(0, 10).cap(1, CAP_IRQ, 2, 0).device(DEV_CONSOLE, 100).irq(2, 10, -1),
            task("echo", user("echo")).send(7, 10, 0),
            task("dorme", sleeper),
        ],
        0,
        None,
    )?;
    let at = s.k("idle_wait_timed");
    let mut guard = 0;
    while !(s.m.kernel && s.m.pc.val() == at) {
        s.m.step();
        guard += 1;
        ensure!(guard < 500_000, "o núcleo nunca chegou à espera ociosa com prazo");
    }
    s.type_text("z q");
    let (stop, _) = run_observed(&mut s, 2_000_000, 1, |_| false)?;
    ensure!(stop == Stop::Idle, "parada {stop:?}");
    ensure!(s.m.console_text() == "Z ", "saída {:?}", s.m.console_text());
    ensure!(s.task("dorme").exit >= 0 && s.task("dorme").state == dead, "o dorminhoco acordou cedo ou não acordou");
    ensure!(s.task("echo").state == dead, "eco não encerrou");
    Ok(())
}

pub fn sc_reply_forgery(k: &Program) -> Result<(), String> {
    // Recibos são números pequenos e previsíveis; o que os torna seguros é
    // o núcleo só aceitar um recibo de quem RECEBEU aquela chamada. Um
    // terceiro tenta responder com todos os recibos plausíveis.
    let server = prog(&format!("{RECV} li a1, SYS_SLEEP\n la a2, 90000\n sys\n li a2, 0\n j exit"));
    let client = prog(
        "li a1, SYS_CALL\n li a2, 0\n li a3, 5\n la t1, 40000\n sys
         mov s1, a1\n mov s2, a3
         li a2, 0\n j exit",
    );
    let mut forger = String::from("    li a1, SYS_SLEEP\n li a2, 9000\n sys\n    li s1, 0\n");
    for h in 1..=40 {
        forger += &format!("    li a1, SYS_REPLY\n li a2, {h}\n li a3, 666\n sys\n    bnp a1, 1f\n    addi s1, s1, 1\n1:\n");
    }
    forger += "    mov a2, s1\n    j exit\n";
    let mut s = boot_k(
        k,
        vec![task("cliente", client).send(0, 1, 0), task("servidor", server).recv(0, 1), task("falsario", prog(&forger))],
        0,
        None,
    )?;
    let stop = run_checked(&mut s, 2_000_000, 1, |_| false)?;
    ensure!(stop == Stop::Halted(0), "parada {stop:?}");
    ensure!(s.task("falsario").exit == 0, "{} resposta(s) forjada(s) aceita(s)", s.task("falsario").exit);
    let c = s.task("cliente");
    ensure!((c.reg(-1), c.reg(-2)) == (0, 5), "o cliente recebeu ({}, {}) — esperado prazo vencido (0) e mensagem intacta", c.reg(-1), c.reg(-2));
    Ok(())
}

pub fn sc_fifo_receivers(k: &Program) -> Result<(), String> {
    // três receptores esperam no mesmo endpoint, chegados na ordem r3, r2,
    // r1; as mensagens 1, 2, 3 vão para eles NESSA ordem
    let receiver = |delay: i64| {
        prog(&format!("li a1, SYS_SLEEP\n li a2, {delay}\n sys\n {RECV} mov a2, a3\n j exit"))
    };
    let sender = prog(
        "li a1, SYS_SLEEP\n la a2, 60000\n sys
         li s1, 1
     1:  li a1, SYS_SEND\n li a2, 0\n mov a3, s1\n li t1, FOREVER\n sys
         addi s1, s1, 1
         li t2, 3
         ble s1, t2, 1b
         li a2, 0\n j exit",
    );
    let mut s = boot_k(
        k,
        vec![
            task("r1", receiver(30000)).recv(0, 6),
            task("r2", receiver(20000)).recv(0, 6),
            task("r3", receiver(10000)).recv(0, 6),
            task("e", sender).send(0, 6, 0),
        ],
        0,
        None,
    )?;
    let stop = run_checked(&mut s, 2_000_000, 1, |_| false)?;
    ensure!(stop == Stop::Halted(0), "parada {stop:?}");
    let got = (s.task("r3").exit, s.task("r2").exit, s.task("r1").exit);
    ensure!(got == (1, 2, 3), "r3, r2, r1 receberam {got:?} (esperado (1, 2, 3))");
    Ok(())
}

pub fn sc_zombie_notices(k: &Program) -> Result<(), String> {
    // Duas tarefas morrem ANTES de o supervisor começar a receber: ficam
    // zumbis. O supervisor colhe exatamente dois avisos, na ordem das
    // mortes; uma terceira recepção (com prazo) não encontra nada.
    let sup = prog(&format!(
        "li a1, SYS_SLEEP\n la a2, 40000\n sys
         {RECV} mov s1, a2\n mov s2, a3\n mov s3, a4\n mov s4, a7\n mov s9, t1
         {RECV} mov s5, a2\n mov s6, a3\n mov s7, a4
         li a1, SYS_RECV\n li a2, 0\n la t1, 5000\n sys
         mov s8, a1
         li a2, 0\n j exit"
    ));
    let mut s = boot_k(
        k,
        vec![
            task("sup", sup).recv(0, 20),
            task("breve", prog("li a2, 42\n j exit")).fault(20, -5),
            task("torta", prog("li a1, SYS_SLEEP\n li a2, 9000\n sys\n ld a1, 0(zr)")).fault(20, -6),
        ],
        0,
        None,
    )?;
    let zombie = s.k("ST_ZOMBIE");
    let mut zombies_seen = 0;
    let stop = run_checked(&mut s, 1_000_000, 1, |s| {
        zombies_seen = zombies_seen.max(s.tasks().iter().filter(|t| t.state == zombie).count());
        false
    })?;
    ensure!(stop == Stop::Halted(0), "parada {stop:?}");
    ensure!(zombies_seen == 2, "zumbis simultâneos vistos: {zombies_seen}");
    let sp = s.task("sup");
    // 1º: selo −5, id 1, causa 0 (exit), código 42, sem recibo; 2º: selo −6, id 2, falha de leitura
    let got = (sp.reg(-1), sp.reg(-2), sp.reg(-3), sp.reg(-4), sp.reg(-9), sp.reg(-5), sp.reg(-6), sp.reg(-7), sp.reg(-8));
    let want = (-5, 1, 0, 42, 0, -6, 2, tenerura_t27::isa::cause::LOAD_FAULT, 0);
    ensure!(got == want, "supervisor viu {got:?}, esperado {want:?}");
    for n in ["breve", "torta"] {
        ensure!(s.task(n).state == s.k("ST_DEAD"), "{n} não descansou depois de colhida");
    }
    Ok(())
}

pub fn sc_driver_restart(k: &Program) -> Result<(), String> {
    // O supervisor MATA o driver de console com um leitor esperando (o
    // leitor acorda com erro e tenta de novo) e depois o REINICIA. A
    // entrada digitada depois do reinício chega normalmente: o serviço
    // voltou sem reiniciar o cliente.
    let reader = prog_drv(
        "    li s1, 0                      ; tentativas que falharam
         1:  call getc
             bp a1, 2f
             addi s1, s1, 1
             li a1, SYS_SLEEP\n li a2, 3000\n sys
             j 1b
         2:  mov s2, a2
             call putc                     ; devolve o caractere lido
             li a2, 0\n j exit",
    );
    let sup = prog(
        "li a1, SYS_SLEEP\n la a2, 20000\n sys
         li a1, SYS_TASK\n li a2, 1\n li a3, -1\n sys\n mov s1, a1     ; mata o driver
         li a1, SYS_SLEEP\n la a2, 20000\n sys
         li a1, SYS_TASK\n li a2, 1\n li a3, 1\n sys\n mov s2, a1      ; reinicia
         li a2, 0\n j exit",
    );
    let mut s = boot_k(
        k,
        vec![
            task("condrv", user("condrv")).recv(0, 10).cap(1, CAP_IRQ, 2, 0).device(DEV_CONSOLE, 100).irq(2, 10, -1),
            task("leitor", reader).send(7, 10, 0),
            task("sup", sup).cap(1, CAP_TASK, 0, 0),
        ],
        0,
        None,
    )?;
    let (stop, _) = run_observed(&mut s, 3_000_000, 1, |_| false)?;
    ensure!(stop == Stop::Idle, "parada {stop:?}");
    ensure!(s.task("condrv").gen == 1, "geração do driver {}", s.task("condrv").gen);
    ensure!(s.task("leitor").state == s.k("ST_BLK_REPLY"), "o leitor deveria estar esperando no driver novo");
    s.type_text("k");
    let (stop, _) = run_observed(&mut s, 1_000_000, 1, |_| false)?;
    ensure!(stop == Stop::Idle, "parada {stop:?}");
    ensure!(s.m.console_text() == "k", "saída {:?}", s.m.console_text());
    let r = s.task("leitor");
    ensure!((r.state, r.exit, r.reg(-2)) == (s.k("ST_DEAD"), 0, 'k' as i64), "leitor: estado {} exit {} leu {}", r.state, r.exit, r.reg(-2));
    ensure!(r.reg(-1) >= 1, "o leitor deveria ter visto ao menos uma falha (E_DEADPEER) quando o driver morreu");
    Ok(())
}

pub fn sc_chaos_mini(k: &Program) -> Result<(), String> {
    for round in 0..4u64 {
        let mut rng = XorShift(0x5EED_2026_1116 ^ 0xC4A0 ^ round.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let (tasks, _) = chaos_tasks(&mut rng, 10);
        let mut s = boot_k(k, tasks, rng.range(200, 3000), Some(rng.next()))?;
        let dead = s.k("ST_DEAD");
        // roda um bom tempo DEPOIS de as vítimas concluírem: o caos continua
        let mut after = 0u32;
        run_checked(&mut s, 1_500_000, 128, |s| {
            if s.task("ping").state == dead && s.task("pong").state == dead {
                after += 1;
            }
            after > 600
        })
        .map_err(|e| format!("rodada {round}: {e}"))?;
        victims_ok(&s).map_err(|e| format!("rodada {round}: {e}"))?;
        ensure!(s.m.console_text() == "ping: soma=65\n", "rodada {round}: saída {:?}", s.m.console_text());
    }
    Ok(())
}

/// Conjunto de tarefas de uma rodada de caos: vítimas (ping com `n`
/// chamadas + pong) no endpoint 1; no endpoint 2, três caóticos de
/// chamadas aleatórias (cada um com capacidade de TAREFA sobre o seguinte:
/// podem matar e reiniciar uns aos outros), um agitador e código
/// aleatório; mais um código aleatório sem capacidade alguma. Embaralhado.
/// Devolve também os nomes dos caóticos.
pub fn chaos_tasks(rng: &mut XorShift, n: i64) -> (Vec<TaskSpec>, Vec<&'static str>) {
    let names = ["fuzz-a", "fuzz-b", "fuzz-c"];
    let mut tasks = vec![
        task("ping", with_word(user("ping"), "count", Word::lit(n))).send(0, 1, 7).con(),
        task("pong", user("pong")).recv(0, 1),
    ];
    for name in names {
        tasks.push(task(name, with_word(user("fuzz"), "seed", rng.word())).send(0, 2, rng.range(0, 5)).recv(1, 2));
    }
    tasks.push(task("kicker", user("kicker")).send(0, 2, 99).recv(1, 2));
    tasks.push(task("rand-a", random_code(rng, 600)).send(0, 2, 1).recv(1, 2));
    tasks.push(task("rand-b", random_code(rng, 600)));
    for i in (1..tasks.len()).rev() {
        let j = rng.range(0, i as i64) as usize;
        tasks.swap(i, j);
    }
    // capacidades de tarefa só depois de embaralhar (o objeto é o índice final)
    for (i, name) in names.iter().enumerate() {
        let target = tasks.iter().position(|t| t.name == names[(i + 1) % 3]).unwrap() as i64;
        let me = tasks.iter().position(|t| t.name == *name).unwrap();
        tasks[me].caps.push(tenerura_t27::sysimage::CapSpec { slot: 2, kind: CAP_TASK, obj: target, badge: 0 });
    }
    (tasks, names.to_vec())
}

pub fn detectors() -> Vec<(&'static str, Detector)> {
    vec![
        ("ping-pong", sc_ping_pong),
        ("preempção+contexto", sc_preempt),
        ("classes de falha", sc_faults),
        ("servidor morto", sc_dead_peer),
        ("pedidos inválidos", sc_invalid_requests),
        ("fifo+selos", sc_fifo_badges),
        ("fifo de receptores", sc_fifo_receivers),
        ("forja de capacidade", sc_cap_forgery),
        ("sleep", sc_sleep),
        ("volta do relógio", sc_clock_wrap),
        ("prazos de IPC", sc_timeouts),
        ("resposta adiada", sc_deferred_reply),
        ("recibo forjado", sc_reply_forgery),
        ("avisos de morte pendentes", sc_zombie_notices),
        ("supervisão", sc_supervision),
        ("reinício limpo", sc_restart_is_fresh),
        ("matar/reiniciar avisa dependentes", sc_kill_and_restart_wake_dependents),
        ("driver+interrupção", sc_driver_irq),
        ("driver morto e reiniciado", sc_driver_restart),
        ("caos", sc_chaos_mini),
        ("autoridade móvel", sc_cap_grant),
    ]
}

/// Fase 6 — autoridade móvel: uma tarefa transfere uma capacidade a outra em
/// execução e a outra passa a poder usá-la; a origem fica vazia. Sem isso,
/// todo direito de uma tarefa teria de ser decidido no boot.
pub fn sc_cap_grant(k: &Program) -> Result<(), String> {
    // (1) quem NÃO nasceu com autoridade recebe a capacidade e a usa.
    let taker = prog(
"    li a1, SYS_RECV
    li a2, 1
    li t1, FOREVER
    sys
    li a1, SYS_PUTC
    li a2, 0
    li a3, 'O'
    sys
    li t2, 1
    bne a1, t2, semcap
    li a2, 42
    j exit
semcap:
    li a2, 99
    j exit",
    );
    let donor = prog(
"    li a1, SYS_CAP_GRANT
    li a2, 7
    li a3, 8
    li a4, 0
    sys
    li t2, 1
    bne a1, t2, recusa
    li a1, SYS_SEND
    li a2, 0
    li a3, 7
    li t1, FOREVER
    sys
    li a2, 0
    j exit
recusa:
    li a2, 90
    j exit",
    );
    let mut s = boot_k(
        k,
        vec![
            task("doador", donor).send(0, 1, 5).cap(7, CAP_TASK, 1, 0).con(),
            task("recebedor", taker).recv(1, 1),
        ],
        0,
        None,
    )?;
    let dead = s.k("ST_DEAD");
    let stop = run_checked(&mut s, 200_000, 64, |s| {
        s.task("doador").state == dead && s.task("recebedor").state == dead
    })?;
    if stop != Stop::Halted(0) {
        return Err(format!("(1) parada inesperada: {stop:?}"));
    }
    if s.task("doador").exit != 0 {
        return Err(format!("(1) doador recusou a transferência (saída {})", s.task("doador").exit));
    }
    if s.task("recebedor").exit != 42 {
        return Err(format!(
            "(1) recebedor não usou a capacidade transferida (saída {})",
            s.task("recebedor").exit
        ));
    }
    if !s.m.console_text().contains('O') {
        return Err("(1) nada foi escrito com a capacidade transferida".into());
    }

    // (2) bordas, na própria tarefa: origem vazia, destino ocupado,
    //     autotransferência para o mesmo slot e a2 que não é cap de tarefa.
    let ego = prog(
"    li a1, SYS_CAP_GRANT
    li a2, 7
    li a3, 8
    li a4, 1
    sys
    li t2, 1
    bne a1, t2, f1
    li a1, SYS_PUTC
    li a2, 1
    li a3, 'X'
    sys
    li t2, 1
    bne a1, t2, f2
    li a1, SYS_PUTC
    li a2, 8
    li a3, 'Y'
    sys
    li t2, -1
    bne a1, t2, f3
    li t2, E_BADCAP
    bne a2, t2, f3
    li a1, SYS_CAP_GRANT
    li a2, 7
    li a3, 8
    li a4, 2
    sys
    li t2, -1
    bne a1, t2, f4
    li t2, E_BADCAP
    bne a2, t2, f4
    li a1, SYS_CAP_GRANT
    li a2, 7
    li a3, 1
    li a4, 1
    sys
    li t2, -1
    bne a1, t2, f5
    li t2, E_NOSLOT
    bne a2, t2, f5
    li a1, SYS_CAP_GRANT
    li a2, 1
    li a3, 1
    li a4, 2
    sys
    li t2, -1
    bne a1, t2, f6
    li t2, E_BADCAP
    bne a2, t2, f6
    li a1, SYS_CAP_GRANT
    li a2, 7
    li a3, 1
    li a4, 9
    sys
    li t2, -1
    bne a1, t2, f7
    li t2, E_BADCAP
    bne a2, t2, f7
    li a1, SYS_CAP_GRANT
    li a2, 7
    li a3, 1
    li a4, -1
    sys
    li t2, -1
    bne a1, t2, f8
    li t2, E_BADCAP
    bne a2, t2, f8
    li a2, 0
    j exit
f1:
    li a2, 71
    j exit
f2:
    li a2, 72
    j exit
f3:
    li a2, 73
    j exit
f4:
    li a2, 74
    j exit
f5:
    li a2, 75
    j exit
f6:
    li a2, 76
    j exit
f7:
    li a2, 77
    j exit
f8:
    li a2, 78
    j exit",
    );
    let mut s = boot_k(k, vec![task("ego", ego).cap(7, CAP_TASK, 0, 0).con()], 0, None)?;
    let dead = s.k("ST_DEAD");
    run_checked(&mut s, 100_000, 64, |s| s.task("ego").state == dead)?;
    if s.task("ego").exit != 0 {
        return Err(format!("(2) borda {} falhou", s.task("ego").exit));
    }
    let out = s.m.console_text();
    if !out.contains('X') || out.contains('Y') {
        return Err(format!("(2) console inesperado: {out:?}"));
    }
    Ok(())
}
