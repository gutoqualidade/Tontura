//! TENERURA — TIR como alvo do sistema (docs/14, 15, 16): intrínsecas
//! privilegiadas na máquina nua, conformidade de ABI, layout de memória,
//! privilégio, proveniência, o módulo do núcleo em dia com a fonte e
//! fuzzing diferencial de programas T27R (gerador de pilha × TIR).

mod common;
use common::*;

use tenerura_t27::asm::assemble_str;
use tenerura_t27::harness::PRELUDE;
use tenerura_t27::lang::{self, ir, Mode};
use tenerura_t27::machine::{Machine, Stop, DEFAULT_RAM_HALF};
use tenerura_t27::word::Word;

/// Camada 0 mínima (docs/14 §5): pilha, vetor de trap, chamada ao código
/// TIR; o resultado vira o código de desligamento. Um trap desliga com a
/// causa (−6 = `brk`).
const BOOT0: &str = "
_start:
    li sp, 0
    la t1, on_trap
    csrw tvec, t1
    call fn_kmain
    la t1, DEV_POWER
    st a1, 0(t1)
1:  j 1b
on_trap:
    csrr a1, cause
    la t1, DEV_POWER
    st a1, 0(t1)
2:  j 2b
";

/// Monta camada 0 + `asm` extra + módulo do núcleo compilado de `src`, roda
/// em modo núcleo e devolve (parada, console).
fn bare(src: &str, asm: &str, data: &str) -> (Stop, String) {
    let lib = lang::compile_kernel(src).unwrap_or_else(|d| panic!("{d}"));
    let full = format!("{PRELUDE}\n{BOOT0}\n{asm}\n{lib}\n    .data\n{data}\n");
    let p = assemble_str(&full, "nua", None).unwrap_or_else(|e| panic!("{e}\n{full}"));
    let mut m = Machine::new(DEFAULT_RAM_HALF);
    for s in &p.segments {
        m.load(s.base, &s.words);
    }
    m.pc = Word::lit(p.entry);
    (m.run(1_000_000), m.console_text())
}

#[test]
fn camada0_entra_no_tir_e_as_intrinsecas_fazem_o_que_a_isa_diz() {
    // MMIO no console, CSR escrita/leitura/troca atômica, tlbi, static com
    // índice dinâmico verificado; 12345 + 12345 + 777
    let (stop, out) = bare(
        "static mut buf: [word; 3];
        fn out(c: word) -> word { phys_write(14348907, c); return 0; }
        fn kmain() -> word {
            out(84); out(73); out(82);
            csr_write(6, 12345);
            let a = csr_read(6);
            let old = csr_swap(6, 777);
            buf[0] = a; buf[1] = old; buf[2] = csr_read(6);
            tlbi();
            let mut s = 0;
            let mut i = 0;
            while i < 3 { s = s + buf[i]; i = i + 1; }
            return s;
        }",
        "",
        "buf: .zero 3",
    );
    assert_eq!(out, "TIR");
    assert_eq!(stop, Stop::Halted(25467));
}

#[test]
fn wfi_espera_o_timer_mesmo_com_interrupcoes_desligadas() {
    // docs/07 §5: wfi conclui com linha pendente e habilitada em IMASK, IE = 0
    let (stop, _) = bare(
        "fn kmain() -> word {
            let t = csr_read(11);
            csr_write(12, t + 50);
            csr_write(8, 1);
            wfi();
            return csr_read(11) - t;
        }",
        "",
        "",
    );
    let Stop::Halted(dt) = stop else { panic!("{stop:?}") };
    assert!((50..60).contains(&dt), "wfi voltou depois de {dt} passos");
}

#[test]
fn indice_fora_do_array_para_no_trap_e_nao_escreve() {
    // índice 5 só conhecido em execução, em [word; 3]: brk (causa −6) ANTES
    // do acesso; o vizinho `guarda` continua intacto
    let (stop, _) = bare(
        "static mut buf: [word; 3];
        static mut guarda: word;
        fn poe(i: word) -> word { buf[i] = 99; return guarda; }
        fn kmain() -> word { return poe(5); }",
        "",
        "buf: .zero 3\nguarda: .word 7",
    );
    assert_eq!(stop, Stop::Halted(-6));
    // índice literal fora: recusado na compilação
    let e = lang::compile_kernel("static mut buf: [word; 3]; fn kmain() -> word { return buf[3]; }").err().unwrap();
    assert!(e.msg.contains("índice 3 fora de 'buf'"), "{}", e.msg);
    // constante que chega pelo SSA (como o `unconditional_panic` do rustc)
    let e = lang::compile_kernel("static mut buf: [word; 3]; fn kmain() -> word { let i = 5; buf[i] = 1; return 0; }").err().unwrap();
    assert!(e.msg.contains("índice 5 fora de 'buf'") && e.msg.contains("conhecido na compilação"), "{}", e.msg);
}

#[test]
fn abi_nove_argumentos_resultado_em_a1_e_registradores_preservados() {
    // docs/14 §2: argumentos a1..a9, resultado a1, s1..s11 e sp preservados
    let asm = "
fn_kmain:
    push lr
    li s1, 101
    li s2, 102
    li s3, 103
    li s4, 104
    li s5, 105
    li s6, 106
    li s7, 107
    li s8, 108
    li s9, 109
    li s10, 110
    li s11, 111
    mov s1, sp              ; sp de antes da chamada
    li a1, 1
    li a2, 2
    li a3, 3
    li a4, 4
    li a5, 5
    li a6, 6
    li a7, 7
    li a8, 8
    li a9, 9
    call fn_f9
    bne sp, s1, ruim
    li t1, 102
    bne s2, t1, ruim
    li t1, 106
    bne s6, t1, ruim
    li t1, 110
    bne s10, t1, ruim
    li t1, 111
    bne s11, t1, ruim
    pop lr
    ret
ruim:
    li a1, -999
    pop lr
    ret
";
    let (stop, _) = bare(
        "fn f9(a: word, b: word, c: word, d: word, e: word, f: word, g: word, h: word, i: word) -> word {
            return a + 3 * b + 9 * c + 27 * d + 81 * e + 243 * f + 729 * g + 2187 * h + 6561 * i;
        }",
        asm,
        "",
    );
    let want: i64 = (1..=9).map(|k| k * 3i64.pow(k as u32 - 1)).sum();
    assert_eq!(stop, Stop::Halted(want));
}

#[test]
fn privilegio_e_recusado_no_modo_errado() {
    // linguagem: cada modo só enxerga as suas embutidas
    let e = lang::compile("fn main() -> word { csr_write(8, 0); return 0; }").err().unwrap();
    assert!(e.msg.contains("não existe em programa de usuário"), "{}", e.msg);
    let e = lang::compile_kernel("fn f() -> word { putc(65); return 0; }").err().unwrap();
    assert!(e.msg.contains("não existe no núcleo"), "{}", e.msg);
    let e = lang::compile_kernel("fn f() -> word { exit(1); return 0; }").err().unwrap();
    assert!(e.msg.contains("não existe no núcleo"), "{}", e.msg);
    let e = lang::compile_kernel("fn f() -> word { csr_write(11, 0); return 0; }").err().unwrap();
    assert!(e.msg.contains("não aceita escrita"), "{}", e.msg);
    // TIR: o mesmo módulo com a flag trocada não valida (nos dois sentidos)
    let mut k = lang::to_ir_mode("fn f() -> word { return csr_read(6); }", Mode::Priv).unwrap();
    k.flags = 0;
    assert!(ir::validate(&k, 1).unwrap_err().contains("não existe em módulo de usuário"));
    let mut u = lang::to_ir("fn main() -> word { putc(65); return 0; }").unwrap();
    u.flags = ir::FLAG_PRIV;
    assert!(ir::validate(&u, 1).unwrap_err().contains("não existe em módulo do núcleo"));
    // CSR só-leitura escrito à mão no TIR
    let mut k = lang::to_ir_mode("fn f() -> word { csr_write(6, 1); return 0; }", Mode::Priv).unwrap();
    let w = k.insts.iter().position(|i| i.op == ir::Op::Intr).unwrap();
    k.insts[w].a = 10;
    assert!(ir::validate(&k, 1).unwrap_err().contains("não aceita escrita"));
}

#[test]
fn proveniencia_deslocamento_constante_fora_da_alocacao_e_recusado() {
    // global de 3 palavras, ptradd 3 → load: o validador recusa sem executar
    let mut m = lang::to_ir_mode("static mut b: [word; 3]; fn f() -> word { return b[2]; }", Mode::Priv).unwrap();
    ir::validate(&m, 1).unwrap();
    let c = m.insts.iter().position(|i| i.op == ir::Op::Const && i.imm() == 2).unwrap();
    m.insts[c].a = 3;
    let e = ir::validate(&m, 1).unwrap_err();
    assert!(e.contains("fora da alocação: deslocamento 3, tamanho 3"), "{e}");
}

#[test]
fn modulo_do_nucleo_em_dia_com_a_fonte_e_com_o_layout_do_nucleo() {
    // o .gen.tas versionado é exatamente o que o compilador gera hoje, com
    // 1 ou várias threads; e cada static cabe no objeto que o núcleo define
    let src = std::fs::read_to_string(os_dir().join("kernel/irq.t27r")).unwrap();
    let gen = std::fs::read_to_string(os_dir().join("kernel/irq.gen.tas")).unwrap();
    let m = lang::to_ir_mode(&src, Mode::Priv).unwrap_or_else(|d| panic!("{d}"));
    let (a, b) = (lang::irgen::generate(&m, 1).unwrap(), lang::irgen::generate(&m, 8).unwrap());
    assert_eq!(a, b);
    assert!(a == gen, "irq.gen.tas desatualizado: rode tools/t27r.sh src/os/kernel/irq.t27r --kernel -o src/os/kernel/irq.gen.tas");
    let k = kernel();
    let mut syms: Vec<(i64, String)> = k.symbols.iter().map(|(n, &v)| (v, n.clone())).collect();
    syms.sort();
    for g in 0..m.globals.len() {
        let name = m.global_name(g);
        let at = k.sym(name);
        // próximo rótulo de dado depois deste: o objeto tem de caber até ele
        let next = syms.iter().map(|(v, _)| *v).filter(|&v| v > at).min().unwrap();
        assert!(next - at >= m.globals[g].size as i64, "static '{name}' declara {} palavras, o núcleo tem {}", m.globals[g].size, next - at);
    }
}

/// Registro de palavras em ordem de declaração (docs/15 §2): deslocamento
/// de cada campo = base + soma dos tamanhos anteriores.
fn layout(base: i64, fields: &[(&str, i64)]) -> Vec<(String, i64)> {
    let mut at = base;
    fields
        .iter()
        .map(|&(n, size)| {
            let r = (n.to_string(), at);
            at += size;
            r
        })
        .collect()
}

#[test]
fn layout_dos_registros_do_nucleo_segue_a_regra_de_docs15() {
    let k = kernel();
    // TCB: 81 palavras, ponteiro no centro (base −40)
    let mut tcb = vec![("T_CAPS", 27), ("T_REGS", 13), ("T_PC", 1), ("T_REGS_POS", 13)];
    let scalars = [
        "T_STATE", "T_PTROOT", "T_EP", "T_SEQ", "T_PEER", "T_TIMED", "T_FCAUSE", "T_FVAL", "T_DEADLINE", "T_ID",
        "T_SENDKIND", "T_BADGE", "T_EXIT", "T_FEP", "T_FBADGE", "T_IRQ", "T_IRQEP", "T_IRQBADGE", "T_IRQPEND", "T_GEN",
    ];
    tcb.extend(scalars.iter().map(|&s| (s, 1)));
    let laid = layout(-40, &tcb);
    for (name, off) in &laid {
        if !name.starts_with("T_REGS") {
            assert_eq!(k.sym(name), *off, "TCB.{name}");
        }
    }
    let end = laid.last().unwrap().1 + 1;
    assert!(end - 1 <= 40 && k.sym("TCB_SIZE") == 81, "TCB passa do centro + 40");
    // capacidade: 3 palavras; 9 delas ocupam T_CAPS
    for (name, off) in layout(0, &[("CAP_KIND", 1), ("CAP_OBJ", 1), ("CAP_BADGE", 1)]) {
        assert_eq!(k.sym(&name), off, "{name}");
    }
    assert_eq!(k.sym("CAP_SLOTS") * 3, 27);
    // entrada da tabela de boot: 15 escalares + 27 de capacidades = BE_SIZE
    let be = [
        "BE_TEXT_PHYS", "BE_TEXT_LEN", "BE_TEXT_VBASE", "BE_DATA_PHYS", "BE_DATA_LEN", "BE_DATA_VBASE", "BE_ENTRY",
        "BE_STACK_PAGES", "BE_FAULT_EP", "BE_FAULT_BADGE", "BE_IRQ_LINE", "BE_IRQ_EP", "BE_IRQ_BADGE", "BE_DEV_PHYS",
        "BE_DEV_VPN",
    ];
    let mut fields: Vec<(&str, i64)> = be.iter().map(|&s| (s, 1)).collect();
    fields.push(("BE_CAPS", 27));
    for (name, off) in layout(0, &fields) {
        assert_eq!(k.sym(&name), off, "{name}");
    }
    assert_eq!(k.sym("BE_SIZE"), 42);
    // as constantes de capacidade do construtor de imagem (Rust) são as do abi.inc
    use tenerura_t27::sysimage::*;
    for (n, v) in [("CAP_EP_SEND", CAP_EP_SEND), ("CAP_EP_RECV", CAP_EP_RECV), ("CAP_CONSOLE", CAP_CONSOLE), ("CAP_TASK", CAP_TASK), ("CAP_IRQ", CAP_IRQ)] {
        assert_eq!(k.sym(n), v, "{n}: Rust × abi.inc");
    }
}

// ── fuzzing diferencial: programas T27R aleatórios nos dois geradores ──

struct Fz {
    rng: XorShift,
    nv: usize,
    /// variáveis visíveis: (nome, mutável)
    vars: Vec<(String, bool)>,
    /// funções já definidas: (nome, aridade) — só se chama para trás (termina)
    fns: Vec<(String, usize)>,
}

impl Fz {
    fn pick(&mut self, n: usize) -> usize {
        self.rng.range(0, n as i64 - 1) as usize
    }

    fn leaf(&mut self) -> String {
        match self.pick(4) {
            0 if !self.vars.is_empty() => {
                let i = self.pick(self.vars.len());
                self.vars[i].0.clone()
            }
            1 => format!("{}", self.rng.range(-3_000_000_000_000, 3_000_000_000_000)),
            _ => format!("{}", self.rng.range(-20, 20)),
        }
    }

    /// expressão `word` (um `trit` entra alargado, docs/18 §4.1)
    fn expr(&mut self, d: u32) -> String {
        let r = self.rng.range(0, 99);
        if d == 0 || r < 30 {
            return self.leaf();
        }
        match r {
            30..=59 => {
                let op = ["+", "-", "*", "/", "%"][self.pick(5)];
                format!("({} {op} {})", self.expr(d - 1), self.expr(d - 1))
            }
            60..=67 => format!("-{}", self.expr(d - 1)),
            68..=81 => self.trit(d - 1),
            82..=88 => format!("shift({}, {})", self.expr(d - 1), self.rng.range(-30, 30)),
            _ if !self.fns.is_empty() => {
                let i = self.pick(self.fns.len());
                let (f, n) = self.fns[i].clone();
                let args: Vec<String> = (0..n).map(|_| self.expr(d - 1)).collect();
                format!("{f}({})", args.join(", "))
            }
            _ => self.expr(d - 1),
        }
    }

    /// expressão `trit`: `sign`, `<=>`, Kleene (`&` = kmin, `|` = kmax), negação
    fn trit(&mut self, d: u32) -> String {
        let r = self.rng.range(0, 99);
        if d == 0 || r < 25 {
            return format!("sign({})", self.leaf());
        }
        match r {
            25..=49 => format!("({} <=> {})", self.expr(d - 1), self.expr(d - 1)),
            50..=74 => {
                let op = ["&", "|"][self.pick(2)];
                let lado = if self.pick(3) == 0 { format!("{}", self.rng.range(-1, 1)) } else { self.trit(d - 1) };
                format!("({} {op} {lado})", self.trit(d - 1))
            }
            75..=87 => format!("-{}", self.trit(d - 1)),
            _ => format!("sign({})", self.expr(d - 1)),
        }
    }

    /// expressão `bool`: a única que serve de condição
    fn cond(&mut self, d: u32) -> String {
        let r = self.rng.range(0, 99);
        if d == 0 || r < 40 {
            let op = ["==", "!=", "<", "<=", ">", ">="][self.pick(6)];
            let d1 = d.saturating_sub(1);
            return format!("({} {op} {})", self.expr(d1), self.expr(d1));
        }
        match r {
            40..=54 => format!("({} && {})", self.cond(d - 1), self.cond(d - 1)),
            55..=69 => format!("({} || {})", self.cond(d - 1), self.cond(d - 1)),
            70..=79 => format!("!{}", self.cond(d - 1)),
            80..=89 => {
                let op = ["&", "|"][self.pick(2)];
                format!("({} {op} {})", self.cond(d - 1), self.cond(d - 1))
            }
            _ => format!("({} == {})", self.trit(d - 1), self.trit(d - 1)),
        }
    }

    fn block(&mut self, d: u32, out: &mut String) {
        let mark = self.vars.len();
        for _ in 0..self.rng.range(1, 4) {
            self.stmt(d, out);
        }
        self.vars.truncate(mark);
    }

    fn stmt(&mut self, d: u32, out: &mut String) {
        let muts: Vec<String> = self.vars.iter().filter(|v| v.1).map(|v| v.0.clone()).collect();
        match self.rng.range(0, 99) {
            0..=24 => {
                self.nv += 1;
                let (name, m) = (format!("v{}", self.nv), self.rng.range(0, 1) == 1);
                // anotado: um `trit` no topo da expressão não pode tipar a variável
                *out += &format!("let {}{name}: word = {};\n", if m { "mut " } else { "" }, self.expr(3));
                self.vars.push((name, m));
            }
            25..=44 if !muts.is_empty() => {
                let v = muts[self.pick(muts.len())].clone();
                *out += &format!("{v} = {};\n", self.expr(3));
            }
            45..=59 if d > 0 => {
                *out += &format!("if {} {{\n", self.cond(2));
                self.block(d - 1, out);
                *out += "} else {\n";
                self.block(d - 1, out);
                *out += "}\n";
            }
            60..=63 if d > 0 => {
                // match de trit exaustivo, sem `_`
                *out += &format!("match {} {{\n", self.trit(2));
                for v in [-1, 0, 1] {
                    *out += &format!("{v} => {{\n");
                    self.block(d - 1, out);
                    *out += "}\n";
                }
                *out += "}\n";
            }
            64..=69 if d > 0 => {
                let (a, b) = (self.rng.range(-2, 2), self.rng.range(3, 9));
                // `+ 0`: o escrutínio é word mesmo quando a expressão é trit
                *out += &format!("match ({} + 0) {{\n{a} => {{\n", self.expr(2));
                self.block(d - 1, out);
                *out += &format!("}}\n{b} | {} => {{\n", b + 1);
                self.block(d - 1, out);
                *out += "}\n_ => {\n";
                self.block(d - 1, out);
                *out += "}\n}\n";
            }
            70..=79 if d > 0 => {
                // laço limitado: o contador não é visível ao corpo
                self.nv += 1;
                let c = format!("c{}", self.nv);
                *out += &format!("let mut {c} = 0;\nwhile {c} < {} {{\n", self.rng.range(0, 4));
                self.block(d - 1, out);
                *out += &format!("{c} = {c} + 1;\n}}\n");
            }
            80..=84 if d > 0 => *out += &format!("if {} {{ return {}; }}\n", self.cond(2), self.expr(2)),
            _ => *out += &format!("putdec({});\nputc(32);\n", self.expr(3)),
        }
    }

    fn program(&mut self) -> String {
        let mut src = String::new();
        for k in 0..self.rng.range(0, 3) {
            let n = self.rng.range(0, 3) as usize;
            let params: Vec<String> = (0..n).map(|i| format!("p{i}")).collect();
            self.vars = params.iter().map(|p| (p.clone(), false)).collect();
            let sig = params.iter().map(|p| format!("{p}: word")).collect::<Vec<_>>().join(", ");
            src += &format!("fn f{k}({sig}) -> word {{\n");
            self.block(2, &mut src);
            src += &format!("return {};\n}}\n", self.expr(3));
            self.fns.push((format!("f{k}"), n));
        }
        self.vars.clear();
        src += "fn main() -> word {\n";
        self.block(3, &mut src);
        src + &format!("return {};\n}}\n", self.expr(3))
    }
}

#[test]
fn fuzzing_diferencial_pilha_x_tir_em_programas_aleatorios() {
    // semente fixa (regra do projeto); cada programa roda sob o núcleo real,
    // com I1–I12, nos dois geradores: console e código de saída iguais
    let (mut instr_pilha, mut instr_tir, n) = (0u64, 0u64, 300);
    for seed in 1..=n {
        let mut fz = Fz { rng: XorShift(0x7151_0000 + seed), nv: 0, vars: vec![], fns: vec![] };
        let src = fz.program();
        let mut seen = Vec::new();
        for c in [lang::compile_pilha as fn(&str) -> _, lang::compile] {
            let tas = c(&src).unwrap_or_else(|d| panic!("semente {seed}: {d}\n{src}"));
            let prog = assemble_str(&tas, "fuzz", Some(&os_dir().join("user"))).unwrap_or_else(|e| panic!("{e}\n{src}"));
            let mut s = boot_k(&kernel(), vec![task("p", prog).con()], 0, None).unwrap();
            let dead = s.k("ST_DEAD");
            run_checked(&mut s, 3_000_000, 256, |s| s.task("p").state == dead).unwrap_or_else(|e| panic!("semente {seed}: {e}\n{src}"));
            assert_eq!(s.task("p").state, dead, "semente {seed}: não terminou\n{src}");
            seen.push((s.m.console_text(), s.task("p").exit, s.task("p").fcause, s.m.stats.instr_user));
        }
        assert_eq!(seen[0].0, seen[1].0, "semente {seed}: console diverge\n{src}");
        assert_eq!((seen[0].1, seen[0].2), (seen[1].1, seen[1].2), "semente {seed}: saída diverge\n{src}");
        instr_pilha += seen[0].3;
        instr_tir += seen[1].3;
    }
    eprintln!("{n} programas aleatórios, 0 divergências; instruções de usuário: pilha {instr_pilha}, tir {instr_tir}");
}
