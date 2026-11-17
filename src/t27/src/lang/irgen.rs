//! TENERURA — TIR → assembly T27.
//!
//! Cada valor tem uma palavra no quadro (`-(slot+1)(s11)`); constantes não
//! ocupam nada e viram imediato onde são usadas. Os valores levados por
//! um `while` (argumentos da condição, do corpo e resultados) dividem as
//! mesmas palavras, então o laço não copia nada na entrada das regiões;
//! só o `yield` do corpo escreve, como cópia paralela. Funções são
//! geradas de forma independente (`jobs` threads) com rótulos próprios
//! (`.L_<função>_<n>`, únicos no programa inteiro porque `fn_<função>` já
//! é); a saída é concatenada em ordem de índice.
//!
//! Módulo com entrada vira programa de usuário (`_start` → `main` →
//! `exit`, + `ulib`). Módulo sem entrada vira biblioteca: só as funções,
//! para `.include` em outro `.tas` — é assim que o núcleo usa TIR (docs/14).
//!
//! Pressupõe módulo validado.

use std::fmt::Write;

use super::gen::{FOOTER, HEADER};
use super::ir::{par_map, BinK, Inst, IntrK, Module, Op, UnK, BIN_NAMES, INTRS, NO_ENTRY};
use crate::isa::csr;
use crate::isa::IMM16_MAX;

/// `writeln!` no texto de saída, sem `String` intermediária por linha
macro_rules! l {
    ($g:expr, $($t:tt)*) => {{
        let _ = writeln!($g.out, $($t)*);
    }};
}

pub fn generate(m: &Module, jobs: usize) -> Result<String, String> {
    let lib = m.entry == NO_ENTRY;
    if !lib && m.name(m.entry as usize) != "main" {
        return Err("a entrada precisa se chamar 'main'".into());
    }
    let parts = par_map(m.funcs.len(), jobs, |f| G::func(m, f));
    let mut out = String::from(if lib { LIB_HEADER } else { HEADER });
    for p in parts {
        out += &p?;
    }
    if !lib {
        out += FOOTER;
    }
    Ok(out)
}

const LIB_HEADER: &str = "\
; ────────────────────────────────────────────────────────────
; gerado por t27r via TIR (biblioteca, docs/14) — não editar
; ────────────────────────────────────────────────────────────
";

struct G<'m> {
    m: &'m Module,
    fi: usize,
    out: String,
    nl: usize,
    slot: Vec<u32>,
    konst: Vec<Option<i64>>,
    /// base (em palavras) da área de cada `slot`, por valor
    area: Vec<u32>,
    /// comparação fundida com o `if`/`cond` seguinte (único uso): vira
    /// `cmp` + desvio, sem materializar 0/1
    fused: Vec<Option<Inst>>,
}

enum Ctx {
    Body,
    /// yield copia para `dst` e segue para `end` (None = cai no fim)
    Branch { dst: Vec<u32>, end: Option<String> },
    Cond { exit: String },
    Loop { dst: Vec<u32>, head: String },
}

impl<'m> G<'m> {
    fn func(m: &'m Module, fi: usize) -> Result<String, String> {
        let f = m.funcs[fi];
        let (ps, _) = m.sig(fi);
        if ps.len() > 9 {
            return Err(format!("@{}: mais de 9 parâmetros (a1..a9)", m.name(fi)));
        }
        let nv = f.nvals as usize;
        let mut g = G { m, fi, out: String::new(), nl: 0, slot: (0..f.nvals).collect(), konst: vec![None; nv], area: vec![0; nv], fused: vec![None; nv] };
        // pré-passada: constantes e apelidos de laço
        let insts = &m.insts[f.ifirst as usize..(f.ifirst + f.icount) as usize];
        let mut alias: Vec<Option<u32>> = vec![None; nv];
        for i in insts {
            match i.op {
                Op::Const => g.konst[i.res as usize] = Some(i.imm()),
                Op::While => {
                    let x = m.list(i.b, 2);
                    let (c, b) = (m.regions[x[0] as usize], m.regions[x[1] as usize]);
                    for k in 0..i.n as u32 {
                        alias[(c.args + k) as usize] = Some(i.res + k);
                        alias[(b.args + k) as usize] = Some(i.res + k);
                    }
                }
                Op::Call if i.n > 9 => return Err(format!("@{}: chamada com mais de 9 argumentos", m.name(fi))),
                _ => {}
            }
        }
        let mut uses = vec![0u32; nv];
        let mut ops = Vec::new();
        for i in insts {
            ops.clear();
            operands(m, i, &mut ops);
            for &v in &ops {
                uses[v as usize] += 1;
            }
        }
        for r in &m.regions[f.rfirst as usize..(f.rfirst + f.rcount) as usize] {
            let rs = &m.insts[r.first as usize..(r.first + r.len) as usize];
            for w in rs.windows(2) {
                let (c, next) = (w[0], w[1]);
                if c.op == Op::Bin && (5..=10).contains(&c.k) && uses[c.res as usize] == 1
                    && matches!(next.op, Op::If | Op::Cond) && next.a == c.res
                {
                    g.fused[c.res as usize] = Some(c);
                }
            }
        }
        // palavra de quadro só para o que é guardado (não fundido): constante não tem,
        // argumento de laço usa a do resultado; depois vêm as áreas de slot
        let keep: Vec<bool> = (0..nv).map(|v| g.konst[v].is_none() && alias[v].is_none() && g.fused[v].is_none()).collect();
        let mut frame = g.place(f.rfirst, 0, &keep);
        for v in 0..nv {
            if let Some(r) = alias[v] {
                g.slot[v] = g.slot[r as usize];
            }
        }
        for i in insts.iter().filter(|i| i.op == Op::Slot) {
            g.area[i.res as usize] = frame;
            frame += i.a;
        }
        let root = m.regions[f.rfirst as usize];
        l!(g, "fn_{}:", m.name(fi));
        g.line("    push lr");
        g.line("    push s11");
        g.line("    mov s11, sp");
        if frame > 0 {
            l!(g, "    addi sp, sp, -{frame}");
        }
        for k in 0..root.nargs {
            g.put(&format!("a{}", k + 1), root.args + k);
        }
        g.region(f.rfirst, &Ctx::Body);
        l!(g, ".Lret_{}:", m.name(fi));
        g.line("    mov sp, s11");
        g.line("    pop s11");
        g.line("    pop lr");
        g.line("    ret");
        Ok(g.out)
    }

    /// Palavras de quadro por escopo: um valor só existe até o fim da região
    /// que o define (docs/13 §C.2), então ramos irmãos e as regiões de um laço
    /// recomeçam na mesma palavra e o quadro é o caminho mais fundo da árvore
    /// de regiões, não a soma. Devolve quantas palavras a subárvore usa.
    fn place(&mut self, r: u32, mut next: u32, keep: &[bool]) -> u32 {
        let m = self.m;
        let reg = m.regions[r as usize];
        let put = |g: &mut Self, v: u32, next: &mut u32| {
            if keep[v as usize] {
                g.slot[v as usize] = *next;
                *next += 1;
            }
        };
        for v in reg.args..reg.args + reg.nargs {
            put(self, v, &mut next);
        }
        let mut top = next;
        for idx in reg.first..reg.first + reg.len {
            let i = m.insts[idx as usize];
            let nres = match i.op {
                Op::If | Op::Switch | Op::While => i.n as u32,
                Op::Call => m.sig(i.a as usize).1.len() as u32,
                Op::Intr => INTRS[i.k as usize].2 as u32,
                Op::Store | Op::Yield | Op::Cond | Op::Return | Op::Exit | Op::Trap => 0,
                _ => 1,
            };
            for v in i.res..i.res + nres {
                put(self, v, &mut next);
            }
            let kids: Vec<u32> = match i.op {
                Op::If | Op::While => m.list(i.b, 2).to_vec(),
                Op::Switch => {
                    let h = m.list(i.b, 3);
                    let (nc, na) = (h[0] as usize, h[1] as usize);
                    let x = m.list(i.b, 3 + 3 * nc + na);
                    std::iter::once(x[2]).chain(x[3 + 3 * nc..].iter().copied()).collect()
                }
                _ => Vec::new(),
            };
            for k in kids {
                top = top.max(self.place(k, next, keep));
            }
            top = top.max(next);
        }
        top
    }

    fn line(&mut self, s: &str) {
        self.out += s;
        self.out.push('\n');
    }

    fn label(&mut self) -> String {
        self.nl += 1;
        format!(".L_{}_{}", self.m.name(self.fi), self.nl)
    }

    fn imm(&mut self, reg: &str, v: i64) {
        let op = if v.abs() <= IMM16_MAX { "li" } else { "la" };
        l!(self, "    {op} {reg}, {v}");
    }

    /// carrega o valor `v` em `reg`
    fn get(&mut self, reg: &str, v: u32) {
        match self.konst[v as usize] {
            Some(c) => self.imm(reg, c),
            None => l!(self, "    ld {reg}, {}(s11)", -(self.slot[v as usize] as i64 + 1)),
        }
    }

    fn put(&mut self, reg: &str, v: u32) {
        self.st(reg, self.slot[v as usize]);
    }

    fn st(&mut self, reg: &str, slot: u32) {
        l!(self, "    st {reg}, {}(s11)", -(slot as i64 + 1));
    }

    /// desvia para `l` se `c` for falso (0); comparação fundida vira um desvio
    fn branch_if_false(&mut self, c: u32, l: &str) {
        match self.fused[c as usize] {
            Some(cmp) => {
                self.get("t1", cmp.a);
                self.get("t2", cmp.b);
                self.line("    cmp t3, t1, t2");
                // falso ⇔ o sinal está fora da classe: eq ne lt le gt ge
                let skip = ["bnz", "bz", "bnn", "bp", "bnp", "bn"][cmp.k as usize - 5];
                l!(self, "    {skip} t3, {l}");
            }
            None => {
                self.get("t1", c);
                l!(self, "    bz t1, {l}");
            }
        }
    }

    /// `dst[i] ← src[i]` para todo i, como se fosse simultâneo
    fn copy(&mut self, src: &[u32], dst: &[u32]) {
        let pairs: Vec<(u32, u32)> = src
            .iter()
            .zip(dst)
            .filter(|(&s, &d)| self.konst[s as usize].is_some() || self.slot[s as usize] != d)
            .map(|(&s, &d)| (s, d))
            .collect();
        let clash = pairs
            .iter()
            .any(|&(s, _)| self.konst[s as usize].is_none() && pairs.iter().any(|&(_, d)| d == self.slot[s as usize]));
        if !clash {
            for (s, d) in pairs {
                self.get("t1", s);
                self.st("t1", d);
            }
            return;
        }
        for &(s, _) in &pairs {
            self.get("t1", s);
            self.line("    push t1");
        }
        for &(_, d) in pairs.iter().rev() {
            self.line("    pop t1");
            self.st("t1", d);
        }
    }

    fn region(&mut self, r: u32, ctx: &Ctx) {
        let m = self.m;
        let reg = m.regions[r as usize];
        for idx in reg.first..reg.first + reg.len {
            let i = m.insts[idx as usize];
            let n = i.n as usize;
            match i.op {
                Op::Const => {}
                Op::Bin if self.fused[i.res as usize].is_some() => {}
                Op::Bin => {
                    self.get("t1", i.a);
                    self.get("t2", i.b);
                    let name = BIN_NAMES[i.k as usize];
                    if i.k <= 4 || i.k >= BinK::Sh as u8 {
                        l!(self, "    {name} a1, t1, t2");
                    } else {
                        // cmp devolve o sinal de (t1 − t2); pula o "1" quando falso
                        let l = self.label();
                        self.line("    cmp t3, t1, t2");
                        self.imm("a1", 0);
                        let skip = ["bnz", "bz", "bnn", "bp", "bnp", "bn"][i.k as usize - 5];
                        l!(self, "    {skip} t3, {l}");
                        self.imm("a1", 1);
                        l!(self, "{l}:");
                    }
                    self.put("a1", i.res);
                }
                Op::Un => {
                    self.get("a1", i.a);
                    match i.k {
                        k if k == UnK::Neg as u8 => self.line("    neg a1, a1"),
                        k if k == UnK::Sgn as u8 => self.line("    sgn a1, a1"),
                        _ => {
                            let l = self.label();
                            self.line("    cmp t1, a1, zr");
                            self.imm("a1", 0);
                            l!(self, "    bnz t1, {l}");
                            self.imm("a1", 1);
                            l!(self, "{l}:");
                        }
                    }
                    self.put("a1", i.res);
                }
                Op::Call => {
                    for (k, &x) in m.list(i.b, n).iter().enumerate() {
                        self.get(&format!("a{}", k + 1), x);
                    }
                    l!(self, "    call fn_{}", m.name(i.a as usize));
                    if !m.sig(i.a as usize).1.is_empty() {
                        self.put("a1", i.res);
                    }
                }
                Op::Intr => self.intr(i.k, i.a, m.list(i.b, n), i.res),
                Op::If => {
                    let x = m.list(i.b, 2);
                    let dst: Vec<u32> = (i.res..i.res + n as u32).map(|v| self.slot[v as usize]).collect();
                    let (l_else, l_end) = (self.label(), self.label());
                    self.branch_if_false(i.a, &l_else);
                    self.region(x[0], &Ctx::Branch { dst: dst.clone(), end: Some(l_end.clone()) });
                    l!(self, "{l_else}:");
                    self.region(x[1], &Ctx::Branch { dst, end: None });
                    l!(self, "{l_end}:");
                }
                Op::Switch => {
                    let h = m.list(i.b, 3);
                    let (nc, na) = (h[0] as usize, h[1] as usize);
                    let x = m.list(i.b, 3 + 3 * nc + na);
                    let dst: Vec<u32> = (i.res..i.res + n as u32).map(|v| self.slot[v as usize]).collect();
                    let labels: Vec<String> = (0..na).map(|_| self.label()).collect();
                    let (l_def, l_end) = (self.label(), self.label());
                    self.get("a1", i.a);
                    for c in x[3..3 + 3 * nc].chunks(3) {
                        self.imm("t1", ((c[1] as u64) << 32 | c[0] as u64) as i64);
                        l!(self, "    beq a1, t1, {}", labels[c[2] as usize]);
                    }
                    l!(self, "    j {l_def}");
                    for (k, &ar) in x[3 + 3 * nc..].iter().enumerate() {
                        l!(self, "{}:", labels[k]);
                        self.region(ar, &Ctx::Branch { dst: dst.clone(), end: Some(l_end.clone()) });
                    }
                    l!(self, "{l_def}:");
                    self.region(x[2], &Ctx::Branch { dst, end: None });
                    l!(self, "{l_end}:");
                }
                Op::While => {
                    let x = m.list(i.b, 2 + n);
                    let dst: Vec<u32> = (i.res..i.res + n as u32).map(|v| self.slot[v as usize]).collect();
                    self.copy(&x[2..], &dst);
                    let (head, exit) = (self.label(), self.label());
                    l!(self, "{head}:");
                    self.region(x[0], &Ctx::Cond { exit: exit.clone() });
                    self.region(x[1], &Ctx::Loop { dst, head });
                    l!(self, "{exit}:");
                }
                Op::Slot => {
                    let base = self.area[i.res as usize] + i.a;
                    l!(self, "    addi a1, s11, -{base}");
                    self.put("a1", i.res);
                }
                Op::PtrAdd => {
                    self.get("t1", i.a);
                    self.get("t2", i.b);
                    self.line("    add a1, t1, t2");
                    self.put("a1", i.res);
                }
                Op::Load => {
                    self.get("t1", i.a);
                    self.line("    ld a1, 0(t1)");
                    self.put("a1", i.res);
                }
                Op::Store => {
                    self.get("t1", i.a);
                    self.get("a1", i.b);
                    self.line("    st a1, 0(t1)");
                }
                Op::Yield => {
                    let src = m.list(i.b, n).to_vec();
                    match ctx {
                        Ctx::Branch { dst, end } => {
                            self.copy(&src, dst);
                            if let Some(e) = end {
                                l!(self, "    j {e}");
                            }
                        }
                        Ctx::Loop { dst, head } => {
                            self.copy(&src, dst);
                            l!(self, "    j {head}");
                        }
                        _ => unreachable!("validado: yield só em ramo ou corpo"),
                    }
                }
                Op::Cond => {
                    let Ctx::Cond { exit } = ctx else { unreachable!("validado") };
                    self.branch_if_false(i.a, exit);
                }
                Op::Return => {
                    if n == 1 {
                        self.get("a1", i.a);
                    }
                    l!(self, "    j .Lret_{}", m.name(self.fi));
                }
                Op::Exit => {
                    self.get("a2", i.a);
                    self.line("    j exit");
                }
                Op::Global => {
                    l!(self, "    la a1, {}", m.global_name(i.a as usize));
                    self.put("a1", i.res);
                }
                // brk: trap síncrono, causa −6 (docs/07). Em usuário a tarefa
                // morre contida; no núcleo vira pânico aninhado.
                // O rótulo na mesma linha marca o trap no listing do montador.
                Op::Trap => {
                    self.nl += 1;
                    l!(self, ".Ltrap_{}_{}: brk", m.name(self.fi), self.nl);
                }
            }
        }
    }

    /// mesma sequência do gerador de pilha, com os argumentos lidos direto
    /// do quadro para os registradores da ABI
    fn intr(&mut self, k: u8, c: u32, args: &[u32], res: u32) {
        const K: [IntrK; 14] = [
            IntrK::Putc,
            IntrK::Putdec,
            IntrK::Yield,
            IntrK::Time,
            IntrK::Call,
            IntrK::Recv,
            IntrK::RecvVal,
            IntrK::CsrRead,
            IntrK::CsrWrite,
            IntrK::CsrSwap,
            IntrK::Wfi,
            IntrK::Tlbi,
            IntrK::PhysRead,
            IntrK::PhysWrite,
        ];
        let csr = csr::name(c as i64).unwrap_or("?");
        let take_a3 = |g: &mut Self| {
            let (l_ok, l_end) = (g.label(), g.label());
            g.line("    li t2, 1");
            l!(g, "    beq a1, t2, {l_ok}");
            l!(g, "    j {l_end}");
            l!(g, "{l_ok}:");
            g.line("    mov a1, a3");
            l!(g, "{l_end}:");
        };
        match K[k as usize] {
            IntrK::Putc | IntrK::Putdec => {
                self.get("a2", args[0]);
                self.line(if k == IntrK::Putc as u8 { "    call putc" } else { "    call putdec" });
                return;
            }
            IntrK::Yield => self.line("    li a1, SYS_YIELD\n    sys"),
            IntrK::Time => self.line("    li a1, SYS_TIME\n    sys"),
            IntrK::Call => {
                self.get("a2", args[0]);
                self.get("a3", args[1]);
                self.get("a4", args[2]);
                self.line("    li t1, -1\n    li a1, SYS_CALL\n    sys");
                take_a3(self);
            }
            IntrK::Recv | IntrK::RecvVal => {
                self.get("a2", args[0]);
                self.get("t1", args[1]);
                self.line("    li a1, SYS_RECV\n    sys");
                if k == IntrK::RecvVal as u8 {
                    take_a3(self);
                }
            }
            IntrK::CsrRead => l!(self, "    csrr a1, {csr}"),
            IntrK::CsrWrite => {
                self.get("t1", args[0]);
                l!(self, "    csrw {csr}, t1");
                return;
            }
            IntrK::CsrSwap => {
                self.get("t1", args[0]);
                l!(self, "    csrx a1, {csr}, t1");
            }
            IntrK::Wfi | IntrK::Tlbi => {
                self.line(if k == IntrK::Wfi as u8 { "    wfi" } else { "    tlbi" });
                return;
            }
            // endereço físico: no núcleo ld/st não passam pela tradução
            IntrK::PhysRead => {
                self.get("t1", args[0]);
                self.line("    ld a1, 0(t1)");
            }
            IntrK::PhysWrite => {
                self.get("t1", args[0]);
                self.get("a1", args[1]);
                self.line("    st a1, 0(t1)");
                return;
            }
        }
        self.put("a1", res);
    }
}

/// valores lidos por uma instrução (para contar usos)
fn operands(m: &Module, i: &Inst, out: &mut Vec<u32>) {
    let n = i.n as usize;
    match i.op {
        Op::Bin | Op::PtrAdd | Op::Store => out.extend([i.a, i.b]),
        Op::Un | Op::Load | Op::If | Op::Switch | Op::Cond | Op::Exit => out.push(i.a),
        Op::Return if n == 1 => out.push(i.a),
        Op::Call | Op::Intr | Op::Yield => out.extend_from_slice(m.list(i.b, n)),
        Op::While => out.extend_from_slice(&m.list(i.b, 2 + n)[2..]),
        _ => {}
    }
}
