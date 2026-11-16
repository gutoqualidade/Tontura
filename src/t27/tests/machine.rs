//! TENERURA T27 — testes da máquina nua (sem núcleo): execução, traps,
//! tradução de endereços e interrupções. Cada teste monta um programa
//! pequeno e observa o estado final — o comportamento verificado é o
//! especificado em docs/07-isa-t27.md.

use tenerura_t27::asm::assemble_str;
use tenerura_t27::harness::{PageTables, PRELUDE};
use tenerura_t27::isa::cause;
use tenerura_t27::machine::*;
use tenerura_t27::word::{Word, M27};

/// Monta `src` (com o prelúdio) e carrega na RAM; devolve a máquina pronta.
fn boot(src: &str) -> Machine {
    let full = format!("{PRELUDE}\n{src}");
    let p = assemble_str(&full, "teste", None).unwrap_or_else(|e| panic!("{e}"));
    let mut m = Machine::new(DEFAULT_RAM_HALF);
    for s in &p.segments {
        m.load(s.base, &s.words);
    }
    m.pc = Word::lit(p.entry);
    m
}

/// Sufixo comum: desliga a máquina com o código em a1.
const HALT: &str = "
halt:   la t1, DEV_POWER
        st a1, 0(t1)
";

#[test]
fn arithmetic_loop_and_halt() {
    // soma 1..100 = 5050
    let mut m = boot(&format!(
        "
        li a1, 0
        li a2, 100
    1:  add a1, a1, a2
        addi a2, a2, -1
        bp a2, 1b
        {HALT}"
    ));
    assert_eq!(m.run(10_000), Stop::Halted(5050));
    assert!(m.stats.instr_kernel > 300 && m.stats.instr_user == 0);
}

#[test]
fn hello_console() {
    let mut m = boot(&format!(
        "
        la s1, msg
        la s2, DEV_CONSOLE
    1:  ld a1, 0(s1)
        bz a1, 2f
        st a1, 0(s2)
        addi s1, s1, 1
        j 1b
    2:  li a1, 0
        {HALT}
        .data
    msg: .stringz \"olá, três\\n\"
        "
    ));
    assert_eq!(m.run(10_000), Stop::Halted(0));
    assert_eq!(m.console_text(), "olá, três\n");
}

#[test]
fn balanced_semantics_visible_at_isa_level() {
    // shi −1 arredonda ao mais próximo; div por 3 coincide; negação exata no
    // extremo da janela; carry ternário com sinal.
    let mut m = boot(&format!(
        "
        li a1, 5
        shi a2, a1, -1          ; 5/3 → 2
        li a3, 3
        div a4, a1, a3          ; 2
        rem a5, a1, a3          ; −1
        la a6, {M27}
        neg a7, a6              ; −M: representável (janela simétrica)
        addc a8, a6, a6         ; M + M estoura para cima: carry +1
        addc a9, a7, a7         ; −M − M: carry −1
        cmp t2, a7, a6          ; −1
        li a1, 0
        {HALT}"
    ));
    assert_eq!(m.run(1000), Stop::Halted(0));
    let r = |n: i8| m.reg(n).val();
    assert_eq!((r(2), r(4), r(5)), (2, 2, -1));
    assert_eq!(r(7), -M27);
    assert_eq!((r(8), r(9), r(11)), (1, -1, -1));
}

#[test]
fn zero_register_and_call_ret() {
    let mut m = boot(&format!(
        "
        la sp, 1000
        li zr, 77               ; escrita em zr é descartada
        li a1, 6
        call dobro
        call dobro
        add a1, a1, zr
        {HALT}
    dobro:
        push lr
        add a1, a1, a1
        pop lr
        ret
        "
    ));
    assert_eq!(m.run(1000), Stop::Halted(24));
    assert_eq!(m.reg(0).val(), 0);
    assert_eq!(m.reg(-13).val(), 1000);
}

#[test]
fn illegal_instruction_traps_precisely() {
    // palavra zero é ilegal; o tratador recebe causa, EPC e a palavra em TVAL
    let mut m = boot(&format!(
        "
        la a1, handler
        csrw tvec, a1
        li a5, 11
    bad: .word 0
        li a5, 22               ; não executa
    handler:
        csrr a1, cause
        csrr a2, epc
        csrr a3, tval
        csrr a4, status
        {HALT}"
    ));
    let bad = 4; // la = 2 palavras, csrw, li
    assert_eq!(m.run(1000), Stop::Halted(cause::ILLEGAL));
    assert_eq!(m.reg(2).val(), bad);
    assert_eq!(m.reg(3).val(), 0);
    assert_eq!(m.reg(5).val(), 11, "instrução após a falha não executou");
    // STATUS: modo núcleo (1), IE=0, PMODE=núcleo (9), PIE=0
    assert_eq!(m.reg(4).val(), 1 + 9);
}

#[test]
fn double_fault_stops_instead_of_spinning() {
    // TVEC aponta para memória zerada: o tratador falha na primeira instrução
    let mut m = boot(
        "
        la a1, 5000
        csrw tvec, a1
        .word 0
        ",
    );
    assert_eq!(m.run(1000), Stop::DoubleFault);
}

/// Prepara um programa de usuário mapeado em páginas e um núcleo mínimo
/// cujo tratador guarda (causa, epc, tval) em a1..a3 e desliga.
/// Layout físico: núcleo em 0.., tabela raiz no quadro 10, tabelas de 2º
/// nível a partir do quadro 11, código de usuário no quadro 20, dados no 21.
fn user_setup(user_src: &str) -> (Machine, PageTables) {
    let kernel = format!(
        "{PRELUDE}
        .org 0
        la a1, handler
        csrw tvec, a1
        la a1, {root}
        csrw ptroot, a1
        la a1, 365              ; entrada do usuário: início da página 1
        csrw epc, a1
        li a1, ST_KERNEL + ST_PIE
        csrw status, a1         ; PMODE = usuário, PIE = 1
        li a1, 0
        eret
    handler:
        csrr a1, cause
        csrr a2, epc
        csrr a3, tval
        la t1, DEV_POWER
        st a1, 0(t1)
        ",
        root = 10 * PAGE_SIZE
    );
    let k = assemble_str(&kernel, "nucleo-minimo", None).unwrap_or_else(|e| panic!("{e}"));
    let u = assemble_str(&format!("{PRELUDE}\n.org 365\n{user_src}"), "usuario", None)
        .unwrap_or_else(|e| panic!("{e}"));
    let mut m = Machine::new(DEFAULT_RAM_HALF);
    m.load(0, &k.text().words);
    let mut pt = PageTables::new(&mut m, 10 * PAGE_SIZE, 11 * PAGE_SIZE);
    // página virtual 1 (código, RX) → quadro 20; página 2 (dados, RW) → quadro 21
    // página −1 (pilha, RW) → quadro 22
    pt.map(&mut m, 1, 20 * PAGE_SIZE, PTE_LEAF + PTE_X);
    pt.map(&mut m, 2, 21 * PAGE_SIZE, PTE_LEAF + PTE_W);
    pt.map(&mut m, -1, 22 * PAGE_SIZE, PTE_LEAF + PTE_W);
    assert!(u.text().words.len() as i64 <= PAGE_SIZE);
    m.load(20 * PAGE_SIZE - PAGE_HALF, &u.text().words);
    if let Some(d) = u.data() {
        assert_eq!(d.base, 2 * PAGE_SIZE - PAGE_HALF);
        m.load(21 * PAGE_SIZE - PAGE_HALF, &d.words);
    }
    (m, pt)
}

#[test]
fn user_mode_runs_translated_and_syscall_traps() {
    let (mut m, _) = user_setup(
        "
        la sp, -364
        li a4, 42
        push a4                 ; pilha na página −1
        la a5, cell
        st a4, 0(a5)            ; dados na página 2
        ld a6, 0(a5)
        pop a7
        sys
        .data
    cell: .word 0
        ",
    );
    assert_eq!(m.run(1000), Stop::Halted(cause::SYSCALL));
    assert_eq!(m.reg(6).val(), 42);
    assert_eq!(m.reg(7).val(), 42);
    // o dado foi parar no quadro físico 21, não no endereço virtual
    assert_eq!(m.peek(21 * PAGE_SIZE - PAGE_HALF).unwrap().val(), 42);
    assert_eq!(m.peek(22 * PAGE_SIZE + PAGE_HALF).unwrap().val(), 42, "topo da pilha");
    assert!(m.stats.instr_user >= 8);
    // EPC do syscall = endereço VIRTUAL da instrução sys
    assert_eq!(m.reg(2).val(), 365 + 2 + 1 + 2 + 2 + 1 + 1 + 2);
}

#[test]
fn user_faults_are_precise_and_classified() {
    // (programa, causa esperada, tval esperado)
    let cases: Vec<(&str, i64, Option<i64>)> = vec![
        // escrita em página de código (sem W)
        ("la a4, 365\n st a4, 0(a4)", cause::STORE_FAULT, Some(365)),
        // leitura de página não mapeada (página 0 = guarda de nulo, centrada em 0)
        ("ld a4, 0(zr)", cause::LOAD_FAULT, Some(0)),
        ("li a4, -5\n ld a4, 0(a4)", cause::LOAD_FAULT, Some(-5)),
        // execução de dados (sem X)
        ("la a4, 1094\n jr a4", cause::FETCH_FAULT, Some(1094)),
        // endereço não canônico (além de 18 trits)
        ("la a4, 193710245\n ld a4, 0(a4)", cause::LOAD_FAULT, Some(193710245)),
        // instruções privilegiadas
        ("csrr a4, status", cause::PRIVILEGE, None),
        ("csrw ptroot, zr", cause::PRIVILEGE, None),
        ("eret", cause::PRIVILEGE, None),
        ("wfi", cause::PRIVILEGE, None),
        // ilegal e breakpoint
        (".word 0", cause::ILLEGAL, Some(0)),
        ("brk", cause::BREAKPOINT, Some(0)),
        // estouro de pilha: página −2 não mapeada
        ("la sp, -1093\n push a4", cause::STORE_FAULT, Some(-1094)),
    ];
    for (src, want, tval) in cases {
        let (mut m, _) = user_setup(src);
        assert_eq!(m.run(1000), Stop::Halted(want), "programa: {src}");
        if let Some(t) = tval {
            assert_eq!(m.reg(3).val(), t, "tval de: {src}");
        }
        // nenhuma das tentativas alterou estado privilegiado
        assert_eq!(m.ptroot.val(), 10 * PAGE_SIZE, "ptroot após: {src}");
    }
}

#[test]
fn user_cannot_reach_physical_memory_outside_its_mapping() {
    // O usuário varre TODO o seu espaço de 18 trits em passos de página
    // tentando ler; só as 3 páginas mapeadas respondem. Em seguida tenta
    // escrever em todas; só as 2 graváveis aceitam. Verificação: a RAM fora
    // dos quadros do usuário fica byte a byte... trit a trit idêntica.
    let (mut m, pt) = user_setup("1: j 1b");
    let user_frames = [20 * PAGE_SIZE, 21 * PAGE_SIZE, 22 * PAGE_SIZE];
    let snapshot: Vec<Word> = (-DEFAULT_RAM_HALF..=DEFAULT_RAM_HALF).map(|a| m.peek(a).unwrap()).collect();
    // entra em modo usuário executando o núcleo mínimo até o ERET
    while m.step() != Step::Eret {}
    assert!(!m.kernel);
    let mut readable = 0;
    let mut writable = 0;
    for vpn in -half_pages()..=half_pages() {
        let va = vpn * PAGE_SIZE;
        if m.translate(va, Access::Load).is_ok() {
            readable += 1;
        }
        if let Ok(pa) = m.translate(va, Access::Store) {
            writable += 1;
            assert!(user_frames.contains(&(page_of(pa) * PAGE_SIZE)));
        }
    }
    assert_eq!((readable, writable), (3, 2));
    let _ = pt;
    let now: Vec<Word> = (-DEFAULT_RAM_HALF..=DEFAULT_RAM_HALF).map(|a| m.peek(a).unwrap()).collect();
    assert!(snapshot == now);
}

fn half_pages() -> i64 {
    page_of(VA_MAX)
}

#[test]
fn timer_interrupt_preempts_user_loop() {
    // usuário em laço infinito; o timer devolve o controle ao núcleo
    let kernel = format!(
        "{PRELUDE}
        .org 0
        la a1, handler
        csrw tvec, a1
        la a1, {root}
        csrw ptroot, a1
        la a1, 365
        csrw epc, a1
        li a1, 500
        csrw timecmp, a1
        li a1, 1
        csrw imask, a1          ; habilita a linha 1 (timer)
        li a1, ST_KERNEL + ST_PIE
        csrw status, a1
        eret
    handler:
        csrr a1, cause
        csrr a2, epc
        csrr a3, time
        csrr a4, status
        la t1, DEV_POWER
        st a1, 0(t1)
        ",
        root = 10 * PAGE_SIZE
    );
    let k = assemble_str(&kernel, "k", None).unwrap();
    let u = assemble_str(".org 365\n1: addi a5, a5, 1\n j 1b", "u", None).unwrap();
    let mut m = Machine::new(DEFAULT_RAM_HALF);
    m.load(0, &k.text().words);
    let mut pt = PageTables::new(&mut m, 10 * PAGE_SIZE, 11 * PAGE_SIZE);
    pt.map(&mut m, 1, 20 * PAGE_SIZE, PTE_LEAF + PTE_X);
    m.load(20 * PAGE_SIZE - PAGE_HALF, &u.text().words);
    assert_eq!(m.run(100_000), Stop::Halted(cause::IRQ_TIMER));
    assert!(m.reg(3).val() >= 500, "interrompeu no tempo programado");
    assert!(m.reg(5).val() > 200, "o usuário chegou a rodar");
    assert!([365, 366].contains(&m.reg(2).val()), "EPC = instrução ainda não executada");
    // STATUS no tratador: núcleo, IE=0, PMODE=usuário, PIE=1
    assert_eq!(m.reg(4).val(), 1 + 27);
}

#[test]
fn wfi_fast_forwards_to_timer_and_idle_is_detected() {
    let mut m = boot(&format!(
        "
        la a1, 100000
        csrw timecmp, a1
        li a1, 1
        csrw imask, a1
        wfi                     ; IE=0: não toma trap, só espera ficar pendente
        csrr a2, irqn
        csrr a3, time
        csrw imask, zr          ; agora nada pode acordar
        wfi
        li a1, 9
        {HALT}"
    ));
    assert_eq!(m.run(1000), Stop::Idle);
    assert_eq!(m.reg(2).val(), 1);
    assert!(m.reg(3).val() >= 100_000);
}

#[test]
fn csr_validation() {
    // STATUS com codificação reservada (trit −1 ou 2) é instrução ilegal
    for bad in ["li a1, -1", "li a1, 2", "li a1, 81", "li a1, 5"] {
        let mut m = boot(&format!(
            "
            la a2, handler
            csrw tvec, a2
            {bad}
            csrw status, a1
            li a1, 0
            j halt
        handler:
            csrr a1, cause
            {HALT}"
        ));
        assert_eq!(m.run(1000), Stop::Halted(cause::ILLEGAL), "{bad}");
    }
    // CSR só de leitura e CSR inexistente
    for bad in ["csrw time, a1", "csrw irqn, a1", "csrw ipend, a1", "csrr a1, 99", "csrw 0, a1"] {
        let mut m = boot(&format!(
            "
            la a2, handler
            csrw tvec, a2
            {bad}
            li a1, 0
            j halt
        handler:
            csrr a1, cause
            {HALT}"
        ));
        assert_eq!(m.run(1000), Stop::Halted(cause::ILLEGAL), "{bad}");
    }
}
