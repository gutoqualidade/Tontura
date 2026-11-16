//! TENERURA — testes de MUTAÇÃO do micronúcleo (controle negativo da
//! própria verificação).
//!
//! Pergunta: se o núcleo tivesse um defeito, a suíte perceberia? Método:
//! plantar um defeito por vez no fonte do núcleo (troca textual única),
//! montar, e rodar os cenários-detectores (tests/common). Cada mutante TEM
//! de ser detectado por pelo menos um cenário; o núcleo original tem de
//! passar em todos. Um mutante sobrevivente é um buraco na suíte.
//!
//! Limitação: ~50 mutantes escolhidos à mão não medem "cobertura de
//! defeitos" em geral; mostram que as classes de defeito listadas são
//! percebidas.

mod common;
use common::*;

use tenerura_t27::asm::{assemble_str, Program};

/// (descrição, trecho original — deve ocorrer exatamente `n` vezes e a
/// troca atinge a ocorrência `which` —, substituto)
struct Mutant {
    what: &'static str,
    old: &'static str,
    which: usize,
    new: &'static str,
}

const fn m(what: &'static str, old: &'static str, which: usize, new: &'static str) -> Mutant {
    Mutant { what, old, which, new }
}

fn mutants() -> Vec<Mutant> {
    vec![
        // ── validação de pedidos ──
        m("chamada sem limite superior", "    bge a1, t1, sys_bad\n", 0, "    nop\n"),
        m("chamada sem limite inferior", "    bn a1, sys_bad\n", 0, "    nop\n"),
        m("send sem conferir o tipo da capacidade", "    li t2, CAP_EP_SEND\n    bne t1, t2, bad_cap\n", 0, "    li t2, CAP_EP_SEND\n    nop\n"),
        m("recv sem conferir o tipo da capacidade", "    li t2, CAP_EP_RECV\n    bne t1, t2, bad_cap\n", 0, "    li t2, CAP_EP_RECV\n    nop\n"),
        m("console sem conferir capacidade", "    li t2, CAP_CONSOLE\n    bne t1, t2, bad_cap\n", 0, "    li t2, CAP_CONSOLE\n    nop\n"),
        m("operação de tarefa sem conferir capacidade", "    li t2, CAP_TASK\n    bne t1, t2, bad_cap\n", 0, "    li t2, CAP_TASK\n    nop\n"),
        m("reconhecimento de irq sem conferir capacidade", "    li t2, CAP_IRQ\n    bne t1, t2, bad_cap\n", 0, "    li t2, CAP_IRQ\n    nop\n"),
        m("slot de capacidade sem limite superior", "    li t1, CAP_SLOTS\n    bge a1, t1, 1f\n", 0, "    li t1, CAP_SLOTS\n    nop\n"),
        m("slot de capacidade sem limite inferior", "cap_lookup:\n    bn a1, 1f\n", 0, "cap_lookup:\n    nop\n"),
        // ── máquina de estados do IPC ──
        m("morte não avisa quem aguardava resposta", "    ld t3, T_PEER(s3)\n    bne t3, s2, 2f\n", 0, "    ld t3, T_PEER(s3)\n    j 2f\n"),
        m("fila de receptores invertida (LIFO)", "    sub t4, t3, a4                  ; carimbos também são modulares: compara pela diferença\n    bnn t4, 2f\n", 0, "    sub t4, t3, a4\n    bnp t4, 2f\n"),
        m("fila de emissores invertida (LIFO)", "4:  ld t3, T_EP(t1)\n    bne t3, a2, 2f\n    ld t3, T_SEQ(t1)\n    bz a3, 3f\n    sub t4, t3, a4\n    bnn t4, 2f\n", 0, "4:  ld t3, T_EP(t1)\n    bne t3, a2, 2f\n    ld t3, T_SEQ(t1)\n    bz a3, 3f\n    sub t4, t3, a4\n    bnp t4, 2f\n"),
        m("selo não entregue (emissor bloqueado)", "    ld t1, T_BADGE(s6)\n    st t1, 2(s1)\n", 0, "    ld t1, T_BADGE(s6)\n    nop\n"),
        m("selo não entregue (receptor esperando)", "    call copy_msg\n    st s3, 2(s6)\n", 0, "    call copy_msg\n    nop\n"),
        m("entrega direta não acorda o receptor", "1:  mov a1, s6\n    li a2, 1\n    call make_ready\n    li t1, 2\n    beq s4, t1, 2f\n", 0, "1:  nop\n    nop\n    nop\n    li t1, 2\n    beq s4, t1, 2f\n"),
        m("mensagem truncada (falta uma palavra)", "    ld t1, 9(a1)\n    st t1, 9(a2)\n", 0, "    nop\n    nop\n"),
        m("chamada não avança o PC", "    ld t1, T_PC(s1)\n    addi t1, t1, 1                  ; retorna após o `sys`\n", 0, "    ld t1, T_PC(s1)\n    nop\n"),
        m("recibo de send simples não zerado", "    st s3, 2(s6)\n    st zr, 10(s6)                   ; send simples: sem recibo\n", 0, "    st s3, 2(s6)\n    nop\n"),
        m("reply aceita recibo de chamador alheio", "    ld t3, T_PEER(t1)\n    bne t3, s1, 2f\n    ld t3, T_SEQ(t1)\n", 0, "    ld t3, T_PEER(t1)\n    nop\n    ld t3, T_SEQ(t1)\n"),
        m("reply não confere o recibo", "    ld t3, T_SEQ(t1)\n    beq t3, a2, 4f\n", 0, "    ld t3, T_SEQ(t1)\n    j 4f\n"),
        // ── prazos ──
        m("prazos nunca vencem", "1:  ld t3, T_TIMED(t1)              ; prazos vencidos: sleep termina com +1;\n    bz t3, 2f", 0, "1:  ld t3, T_TIMED(t1)\n    j 2f"),
        m("paciência ignorada (espera sempre para sempre)", "    st zr, T_TIMED(s1)\n    bn t1, 9f\n", 0, "    st zr, T_TIMED(s1)\n    j 9f\n"),
        m("prazo não desarmado ao acordar por evento", "    st t1, T_STATE(a1)\n    st zr, T_TIMED(a1)\n", 0, "    st t1, T_STATE(a1)\n    nop\n"),
        m("carimbo pode ser zero", "1:  addi t2, t2, 1\n    bz t2, 1b\n    st t2, 0(t1)\n    st t2, T_SEQ(a1)\n", 0, "1:  addi t2, t2, 1\n    nop\n    st t2, 0(t1)\n    st t2, T_SEQ(a1)\n"),
        m("paciência zero bloqueia a recepção", "    ld t1, 10(s1)\n    bz t1, ret_zero                 ; paciência zero: não espera\n    li t1, ST_BLK_RECV\n", 0, "    ld t1, 10(s1)\n    nop\n    li t1, ST_BLK_RECV\n"),
        m("paciência zero bloqueia o envio", "    ld t1, 10(s1)\n    bz t1, ret_zero                 ; paciência zero: não espera\n    li t1, ST_BLK_SEND\n", 0, "    ld t1, 10(s1)\n    nop\n    li t1, ST_BLK_SEND\n"),
        m("sleep sem teto", "    ble t1, t2, 1f\n    mov t1, t2\n1:  csrr t2, time\n    add t1, t1, t2\n    st t1, T_DEADLINE(s1)\n    li t1, 1\n    st t1, T_TIMED(s1)\n    li t1, ST_SLEEP\n", 0, "    ble t1, t2, 1f\n    nop\n1:  csrr t2, time\n    add t1, t1, t2\n    st t1, T_DEADLINE(s1)\n    li t1, 1\n    st t1, T_TIMED(s1)\n    li t1, ST_SLEEP\n"),
        m("vencimento por comparação direta (não modular)", "    sub t3, a3, t3                  ; agora − prazo: o relógio é modular, o SINAL decide\n    bn t3, 2f\n", 0, "    nop\n    blt a3, t3, 2f\n"),
        // ── troca de contexto ──
        m("registrador não salvo na entrada", "    st s7, -7(t4)\n", 0, "    nop\n"),
        m("registrador não restaurado na saída", "    ld a5, 5(t4)\n", 0, "    nop\n"),
        m("t4 do usuário não salvo", "    csrr a1, scratch\n    st a1, 13(t4)                   ; t4 do usuário\n", 0, "    csrr a1, scratch\n    nop\n"),
        // ── memória ──
        m("pilha mapeada com execução", "    mov a2, s9\n    li a3, PTE_RW\n", 0, "    mov a2, s9\n    li a3, 13\n"),
        m("alocador devolve sempre o mesmo quadro", "    addi t3, a1, PAGE_SIZE\n    st t3, 0(t1)\n", 0, "    addi t3, a1, PAGE_SIZE\n    nop\n"),
        m("quadros não são zerados", "1:  st zr, 0(t1)\n    addi t1, t1, 1\n    ble t1, t2, 1b\n    ret\n", 0, "1:  nop\n    addi t1, t1, 1\n    ble t1, t2, 1b\n    ret\n"),
        // ── tempo e escalonamento ──
        m("fatia nunca renovada", "    add t1, t1, t3\n    csrw timecmp, t1                ; nova fatia\n", 0, "    add t1, t1, t3\n    nop\n"),
        m("timer nunca habilitado", "    st t1, 0(t2)\n    csrw imask, t1\n\n    li s4, 0\n", 0, "    st t1, 0(t2)\n    nop\n\n    li s4, 0\n"),
        // ── morte, supervisão, reinício ──
        m("aviso de morte nunca enviado", "    ld s2, T_FEP(s1)\n    bz s2, schedule\n", 0, "    ld s2, T_FEP(s1)\n    j schedule\n"),
        m("zumbi colhido continua zumbi", "    call deliver_death\n    li t1, ST_DEAD\n    st t1, T_STATE(s6)\n", 0, "    call deliver_death\n    li t1, ST_DEAD\n    nop\n"),
        m("reinício não avisa dependentes", "task_restart:\n    mov a1, s3                      ; quem dependia da encarnação antiga é avisado\n    call kill_task\n", 0, "task_restart:\n    mov a1, s3\n    nop\n"),
        m("reinício não recarrega os dados", "    ld a4, BE_DATA_VBASE(s6)\n    li a5, PTE_RW\n    call load_segment\n", 0, "    ld a4, BE_DATA_VBASE(s6)\n    li a5, PTE_RW\n    nop\n"),
        m("reinício não zera quadros reaproveitados", "    shi a1, t1, 6                   ; é arredondar ao múltiplo de 729 mais próximo\n    call zero_frame\n", 0, "    shi a1, t1, 6\n    nop\n"),
        m("reinício não zera os registradores", "5:  st zr, 0(t1)\n    addi t1, t1, 1\n    ble t1, t2, 5b\n", 0, "5:  nop\n    addi t1, t1, 1\n    ble t1, t2, 5b\n"),
        m("reinício não conta a geração", "    ld t1, T_GEN(s3)\n    addi t1, t1, 1\n    st t1, T_GEN(s3)\n", 0, "    ld t1, T_GEN(s3)\n    nop\n    st t1, T_GEN(s3)\n"),
        m("matar não mata", "    st zr, T_EXIT(s3)\n    mov a1, s3\n    call kill_task\n", 0, "    st zr, T_EXIT(s3)\n    mov a1, s3\n    nop\n"),
        // ── interrupções ──
        m("linha não mascarada ao entregar a interrupção", "    mov s2, a1\n    call irq_disable\n    la s3, TCB_FIRST\n", 0, "    mov s2, a1\n    nop\n    la s3, TCB_FIRST\n"),
        m("reconhecimento não reabilita a linha", "    ld a1, CAP_OBJ(a1)\n    call irq_enable\n", 0, "    ld a1, CAP_OBJ(a1)\n    nop\n"),
        m("interrupção pendente nunca entregue no recv", "    ld t1, T_IRQPEND(s1)            ; interrupção pendente do próprio driver vem primeiro\n    bz t1, 1f\n", 0, "    ld t1, T_IRQPEND(s1)\n    j 1f\n"),
        m("ocioso desliga apesar de haver driver vivo", "7:  bz a5, 8f\n", 0, "7:  j 8f\n"),
        m("driver morto mantém a linha habilitada", "    ld a1, T_IRQ(s2)                ; driver morto: a linha dele fica mascarada\n    call irq_disable\n", 0, "    ld a1, T_IRQ(s2)\n    nop\n"),
        m("dispositivo mapeado com execução", "    ld a2, BE_DEV_VPN(s6)\n    li a4, PTE_RW\n", 0, "    ld a2, BE_DEV_VPN(s6)\n    li a4, 13\n"),
    ]
}

fn kernel_source() -> String {
    std::fs::read_to_string(os_dir().join("kernel/kernel.tas")).unwrap()
}

fn build_mutant(src: &str, mu: &Mutant) -> Program {
    let hits: Vec<usize> = src.match_indices(mu.old).map(|(i, _)| i).collect();
    assert!(hits.len() > mu.which, "mutante '{}': trecho não encontrado no núcleo (o fonte mudou?)", mu.what);
    assert_eq!(hits.len(), mu.which + 1, "mutante '{}': trecho ambíguo ({} ocorrências)", mu.what, hits.len());
    let at = hits[mu.which];
    let mutated = format!("{}{}{}", &src[..at], mu.new, &src[at + mu.old.len()..]);
    assemble_str(&mutated, "kernel-mutante", Some(&os_dir().join("kernel")))
        .unwrap_or_else(|e| panic!("mutante '{}' não monta: {e}", mu.what))
}

/// Primeiro detector que falha para este núcleo (ou None se todos passam).
fn first_failure(k: &Program) -> Option<(String, String)> {
    for (name, f) in detectors() {
        let r = std::panic::catch_unwind(|| f(k));
        match r {
            Ok(Ok(())) => {}
            Ok(Err(e)) => return Some((name.to_string(), e)),
            Err(_) => return Some((name.to_string(), "pânico no arnês".to_string())),
        }
    }
    None
}

#[test]
fn real_kernel_passes_every_detector() {
    let k = kernel();
    for (name, f) in detectors() {
        f(&k).unwrap_or_else(|e| panic!("detector '{name}' falhou no núcleo real: {e}"));
    }
}

#[test]
fn every_planted_kernel_defect_is_detected() {
    let src = kernel_source();
    let list = mutants();
    // silencia as mensagens de pânico esperadas dentro dos detectores
    std::panic::set_hook(Box::new(|_| {}));
    let results: Vec<(String, Option<(String, String)>)> = std::thread::scope(|sc| {
        let handles: Vec<_> = list
            .iter()
            .map(|mu| {
                let src = &src;
                sc.spawn(move || {
                    let k = build_mutant(src, mu);
                    (mu.what.to_string(), first_failure(&k))
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let _ = std::panic::take_hook();
    let mut survivors = Vec::new();
    for (what, r) in &results {
        match r {
            Some((det, why)) => eprintln!("mutante DETECTADO  {what:<48} ← {det}: {}", why.lines().next().unwrap_or("")),
            None => {
                eprintln!("mutante SOBREVIVEU {what}");
                survivors.push(what.clone());
            }
        }
    }
    assert!(survivors.is_empty(), "{} mutante(s) não detectado(s): {survivors:?}", survivors.len());
}
