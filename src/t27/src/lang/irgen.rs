//! TENERURA — TIR → assembly T27.
//!
//! Cada valor tem uma palavra no quadro (`-(slot+1)(s11)`); constantes não
//! ocupam nada e viram imediato onde são usadas. Os valores levados por
//! um `while` (argumentos da condição, do corpo e resultados) dividem as
//! mesmas palavras, então o laço não copia nada na entrada das regiões;
//! só o `yield` do corpo escreve, como cópia paralela. Funções são
//! geradas de forma independente (`jobs` threads) com rótulos próprios
//! (`.L<função>_<n>`); a saída é concatenada em ordem de índice.
//!
//! Pressupõe módulo validado.

use std::fmt::Write;

use super::gen::{FOOTER, HEADER};
use super::ir::{par_map, IntrK, Module, Op, UnK, BIN_NAMES};
use crate::isa::IMM16_MAX;

/// `writeln!` no texto de saída, sem `String` intermediária por linha
macro_rules! l {
    ($g:expr, $($t:tt)*) => {{
        let _ = writeln!($g.out, $($t)*);
    }};
}

pub fn generate(m: &Module, jobs: usize) -> Result<String, String> {
    if m.name(m.entry as usize) != "main" {
        return Err("a entrada precisa se chamar 'main'".into());
    }
    let parts = par_map(m.funcs.len(), jobs, |f| G::func(m, f));
    let mut out = String::from(HEADER);
    for p in parts {
        out += &p?;
    }
    out += FOOTER;
    Ok(out)
}

struct G<'m> {
    m: &'m Module,
    fi: usize,
    out: String,
    nl: usize,
    slot: Vec<u32>,
    konst: Vec<Option<i64>>,
    /// base (em palavras) da área de cada `slot`, por valor
    area: Vec<u32>,
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
        let mut g = G { m, fi, out: String::new(), nl: 0, slot: (0..f.nvals).collect(), konst: vec![None; nv], area: vec![0; nv] };
        // pré-passada: constantes, apelidos de laço, áreas de slot
        let mut frame = f.nvals;
        for i in &m.insts[f.ifirst as usize..(f.ifirst + f.icount) as usize] {
            match i.op {
                Op::Const => g.konst[i.res as usize] = Some(i.imm()),
                Op::While => {
                    let x = m.list(i.b, 2);
                    let (c, b) = (m.regions[x[0] as usize], m.regions[x[1] as usize]);
                    for k in 0..i.n as u32 {
                        g.slot[(c.args + k) as usize] = i.res + k;
                        g.slot[(b.args + k) as usize] = i.res + k;
                    }
                }
                Op::Slot => {
                    g.area[i.res as usize] = frame;
                    frame += i.a;
                }
                Op::Call if i.n > 9 => return Err(format!("@{}: chamada com mais de 9 argumentos", m.name(fi))),
                _ => {}
            }
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
        l!(g, ".Lret{}:", fi);
        g.line("    mov sp, s11");
        g.line("    pop s11");
        g.line("    pop lr");
        g.line("    ret");
        Ok(g.out)
    }

    fn line(&mut self, s: &str) {
        self.out += s;
        self.out.push('\n');
    }

    fn label(&mut self) -> String {
        self.nl += 1;
        format!(".L{}_{}", self.fi, self.nl)
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
                Op::Bin => {
                    self.get("t1", i.a);
                    self.get("t2", i.b);
                    let name = BIN_NAMES[i.k as usize];
                    if i.k <= 4 {
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
                Op::Intr => self.intr(i.k, m.list(i.b, n), i.res),
                Op::If => {
                    let x = m.list(i.b, 2);
                    let dst: Vec<u32> = (i.res..i.res + n as u32).map(|v| self.slot[v as usize]).collect();
                    let (l_else, l_end) = (self.label(), self.label());
                    self.get("t1", i.a);
                    l!(self, "    bz t1, {l_else}");
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
                    let dst: Vec<u32> = (i.res..i.res + n as u32).collect();
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
                    self.get("t1", i.a);
                    l!(self, "    bz t1, {exit}");
                }
                Op::Return => {
                    if n == 1 {
                        self.get("a1", i.a);
                    }
                    l!(self, "    j .Lret{}", self.fi);
                }
                Op::Exit => {
                    self.get("a2", i.a);
                    self.line("    j exit");
                }
            }
        }
    }

    /// mesma sequência do gerador de pilha, com os argumentos lidos direto
    /// do quadro para os registradores da ABI
    fn intr(&mut self, k: u8, args: &[u32], res: u32) {
        const K: [IntrK; 7] =
            [IntrK::Putc, IntrK::Putdec, IntrK::Yield, IntrK::Time, IntrK::Call, IntrK::Recv, IntrK::RecvVal];
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
        }
        self.put("a1", res);
    }
}
