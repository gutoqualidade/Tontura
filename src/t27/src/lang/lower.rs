//! TENERURA — tradução direta T27R → TIR, em uma passada.
//!
//! Com controle estruturado, SSA sai sem análise de dominância: cada
//! variável é só "o valor corrente" num ambiente. Antes de um `if`, `match`
//! ou `while`, uma varredura sintática barata (`assigned`) diz quais
//! variáveis de fora o comando reatribui; essas — e só essas — viram
//! resultados do comando (e argumentos das regiões do laço). Não há PHI
//! para inserir nem para limpar depois.
//!
//! Acesso `a[i]` a um `static` vira verificação de limite explícita
//! (`if i < 0 { trap }`, `if i >= n { trap }`) seguida de `ptradd`; com
//! índice literal a checagem já provou o limite e o validador o reconfere
//! pela proveniência. Nenhum programa T27R aceito alcança UB de memória.
//!
//! Pressupõe programa aceito por `check` no mesmo modo.

use std::collections::{BTreeSet, HashMap};

use super::ir::{BinK, Func, Global, Inst, IntrK, Mode, Module, Op, Region, Ty, UnK, FLAG_PRIV, NO_ENTRY};
use super::parse::{BinOp, Block, Expr, Pat, Program, Stmt, UnOp};

struct Rb {
    args: u32,
    nargs: u32,
    insts: Vec<Inst>,
    done: bool,
}

struct L<'p> {
    m: Module,
    fids: HashMap<&'p str, u32>,
    /// static → (índice do global, tamanho, é array)
    statics: HashMap<&'p str, (u32, i64, bool)>,
    regs: Vec<Rb>,
    rfirst: u32,
    cur: usize,
    nvals: u32,
    /// variável → valor corrente; a posição é a identidade da variável
    env: Vec<(&'p str, u32)>,
    /// valores que são `const` (o SSA leva literais por `let`/atribuição)
    consts: HashMap<u32, i64>,
    /// primeiro índice constante fora do array (vira erro de compilação)
    err: Option<String>,
}

/// Erro só em um caso: índice conhecido na compilação e fora do array —
/// como o `unconditional_panic` do rustc. O TIR nunca contém acesso com
/// deslocamento constante fora da alocação, nem em código morto.
pub fn lower(p: &Program, mode: Mode) -> Result<Module, String> {
    let mut l = L {
        m: Module::default(),
        fids: p.fns.iter().enumerate().map(|(i, f)| (f.name.as_str(), i as u32)).collect(),
        statics: p.statics.iter().enumerate().map(|(i, s)| (s.name.as_str(), (i as u32, s.size, s.array))).collect(),
        regs: Vec::new(),
        rfirst: 0,
        cur: 0,
        nvals: 0,
        env: Vec::new(),
        consts: HashMap::new(),
        err: None,
    };
    // assinaturas primeiro: chamadas para frente usam o índice da callee
    let sigs: Vec<u32> = p
        .fns
        .iter()
        .map(|f| {
            let n = f.params.len() as u32;
            l.list(&[n, 1].into_iter().chain((0..=n).map(|_| Ty::Word as u32)).collect::<Vec<_>>())
        })
        .collect();
    for s in &p.statics {
        let name = l.m.strings.len() as u32;
        l.m.strings.extend_from_slice(s.name.as_bytes());
        l.m.globals.push(Global { name, name_len: s.name.len() as u32, size: s.size as u32 });
    }
    for (f, sig) in p.fns.iter().zip(sigs) {
        l.func(f, sig);
    }
    // núcleo: biblioteca privilegiada; usuário: programa com entrada `main`
    (l.m.flags, l.m.entry) = match mode {
        Mode::Priv => (FLAG_PRIV, NO_ENTRY),
        Mode::User => (0, l.fids["main"]),
    };
    match l.err {
        Some(e) => Err(e),
        None => Ok(l.m),
    }
}

impl<'p> L<'p> {
    fn list(&mut self, xs: &[u32]) -> u32 {
        if xs.is_empty() {
            return 0;
        }
        self.m.extra.extend_from_slice(xs);
        (self.m.extra.len() - xs.len()) as u32
    }

    fn vals(&mut self, n: u32) -> u32 {
        self.nvals += n;
        self.nvals - n
    }

    fn region(&mut self, nargs: u32) -> usize {
        let args = self.vals(nargs);
        self.regs.push(Rb { args, nargs, insts: Vec::new(), done: false });
        self.regs.len() - 1
    }

    fn emit(&mut self, i: Inst) {
        let r = &mut self.regs[self.cur];
        r.done = i.op.is_term();
        r.insts.push(i);
    }

    /// emite uma instrução de um resultado e devolve o valor
    fn val(&mut self, op: Op, k: u8, a: u32, b: u32) -> u32 {
        let res = self.vals(1);
        self.emit(Inst { op, k, n: 0, res, a, b });
        res
    }

    fn konst(&mut self, v: i64) -> u32 {
        let x = self.val(Op::Const, 0, v as u64 as u32, (v as u64 >> 32) as u32);
        self.consts.insert(x, v);
        x
    }

    fn bin(&mut self, k: BinK, a: u32, b: u32) -> u32 {
        self.val(Op::Bin, k as u8, a, b)
    }

    fn yield_(&mut self, xs: &[u32]) {
        let b = self.list(xs);
        self.emit(Inst { op: Op::Yield, n: xs.len() as u16, b, ..Inst::new(Op::Yield) });
    }

    /// variável local (posição em `env`); `None` = é um static
    fn lookup(&self, name: &str) -> Option<usize> {
        self.env.iter().rposition(|(n, _)| *n == name)
    }

    /// `if c { trap } else {}` — o caminho de falha de uma verificação
    fn trap_if(&mut self, c: u32) {
        let parent = self.cur;
        self.cur = self.region(0);
        self.emit(Inst::new(Op::Trap));
        let rt = self.rfirst + self.cur as u32;
        self.cur = self.region(0);
        self.yield_(&[]);
        let re = self.rfirst + self.cur as u32;
        self.cur = parent;
        self.structured(Op::If, c, &[rt, re], &[]);
    }

    /// endereço de `nome[i]`, com verificação de limite se `i` não é constante
    fn element(&mut self, name: &str, index: &'p Expr) -> u32 {
        let (g, size, _) = self.statics[name];
        let i = self.expr(index);
        if let Some(&c) = self.consts.get(&i) {
            if !(0..size).contains(&c) && self.err.is_none() {
                self.err = Some(format!("índice {c} fora de '{name}' ([word; {size}]), conhecido na compilação"));
            }
        } else {
            let z = self.konst(0);
            let lo = self.bin(BinK::Lt, i, z);
            self.trap_if(lo);
            let n = self.konst(size);
            let hi = self.bin(BinK::Ge, i, n);
            self.trap_if(hi);
        }
        let base = self.val(Op::Global, 0, g, 0);
        self.val(Op::PtrAdd, 0, base, i)
    }

    fn func(&mut self, f: &'p super::parse::Fn, sig: u32) {
        self.regs.clear();
        self.env.clear();
        self.nvals = 0;
        self.rfirst = self.m.regions.len() as u32;
        self.cur = self.region(f.params.len() as u32);
        for (i, p) in f.params.iter().enumerate() {
            self.env.push((p.as_str(), i as u32));
        }
        self.stmts(&f.body.stmts);
        if !self.regs[self.cur].done {
            let z = self.konst(0);
            self.emit(Inst { op: Op::Return, n: 1, a: z, ..Inst::new(Op::Return) });
        }
        let ifirst = self.m.insts.len() as u32;
        for r in self.regs.drain(..) {
            let first = self.m.insts.len() as u32;
            self.m.regions.push(Region { args: r.args, nargs: r.nargs, first, len: r.insts.len() as u32 });
            self.m.insts.extend(r.insts);
        }
        let name = self.m.strings.len() as u32;
        self.m.strings.extend_from_slice(f.name.as_bytes());
        self.m.funcs.push(Func {
            name,
            name_len: f.name.len() as u32,
            sig,
            rfirst: self.rfirst,
            rcount: self.m.regions.len() as u32 - self.rfirst,
            ifirst,
            icount: self.m.insts.len() as u32 - ifirst,
            nvals: self.nvals,
        });
    }

    fn stmts(&mut self, ss: &'p [Stmt]) {
        let mark = self.env.len();
        for s in ss {
            if self.regs[self.cur].done {
                break; // depois de return/exit: código morto, não entra no TIR
            }
            self.stmt(s);
        }
        self.env.truncate(mark);
    }

    /// Variáveis de fora (posições em `env`) reatribuídas pelos blocos.
    fn assigned(&self, blocks: &[&'p Block]) -> Vec<usize> {
        fn scan<'p>(l: &L<'p>, ss: &'p [Stmt], shadow: &mut Vec<&'p str>, out: &mut BTreeSet<usize>) {
            let mark = shadow.len();
            for s in ss {
                match s {
                    Stmt::Let { name, .. } => shadow.push(name.as_str()),
                    Stmt::Assign { name, .. } if !shadow.contains(&name.as_str()) => {
                        out.extend(l.lookup(name)); // static não é variável do ambiente
                    }
                    Stmt::If { then, els, .. } => {
                        scan(l, &then.stmts, shadow, out);
                        if let Some(e) = els {
                            scan(l, &e.stmts, shadow, out);
                        }
                    }
                    Stmt::While { body, .. } => scan(l, &body.stmts, shadow, out),
                    Stmt::Match { arms, .. } => arms.iter().for_each(|a| scan(l, &a.body.stmts, shadow, out)),
                    _ => {}
                }
            }
            shadow.truncate(mark);
        }
        let mut out = BTreeSet::new();
        for b in blocks {
            scan(self, &b.stmts, &mut Vec::new(), &mut out);
        }
        out.into_iter().collect()
    }

    /// Constrói uma região de ramo partindo dos valores `saved` das
    /// variáveis levadas; termina com `yield` delas se não sair antes.
    fn branch(&mut self, b: Option<&'p Block>, carried: &[usize], saved: &[u32]) -> u32 {
        for (&c, &v) in carried.iter().zip(saved) {
            self.env[c].1 = v;
        }
        let parent = self.cur;
        self.cur = self.region(0);
        if let Some(b) = b {
            self.stmts(&b.stmts);
        }
        if !self.regs[self.cur].done {
            let xs: Vec<u32> = carried.iter().map(|&c| self.env[c].1).collect();
            self.yield_(&xs);
        }
        let r = self.rfirst + self.cur as u32;
        self.cur = parent;
        r
    }

    /// Emite um comando estruturado com `n` resultados e rebatiza as
    /// variáveis levadas com eles.
    fn structured(&mut self, op: Op, a: u32, head: &[u32], carried: &[usize]) {
        let n = carried.len();
        let ts = std::iter::repeat_n(Ty::Word as u32, n);
        let b = self.list(&head.iter().copied().chain(ts).collect::<Vec<_>>());
        let res = self.vals(n as u32);
        self.emit(Inst { op, k: 0, n: n as u16, res, a, b });
        for (i, &c) in carried.iter().enumerate() {
            self.env[c].1 = res + i as u32;
        }
    }

    fn stmt(&mut self, s: &'p Stmt) {
        match s {
            Stmt::Let { name, init, .. } => {
                let v = self.expr(init);
                self.env.push((name.as_str(), v));
            }
            Stmt::Assign { name, value, .. } => {
                let v = self.expr(value);
                match self.lookup(name) {
                    Some(i) => self.env[i].1 = v,
                    None => {
                        let p = self.val(Op::Global, 0, self.statics[name.as_str()].0, 0);
                        self.emit(Inst { op: Op::Store, a: p, b: v, ..Inst::new(Op::Store) });
                    }
                }
            }
            Stmt::Store { name, index, value, .. } => {
                // como em Rust: o valor atribuído é avaliado antes do lugar
                let v = self.expr(value);
                let p = self.element(name, index);
                self.emit(Inst { op: Op::Store, a: p, b: v, ..Inst::new(Op::Store) });
            }
            Stmt::If { cond, then, els } => {
                let c = self.expr(cond);
                let mut bs = vec![then];
                bs.extend(els);
                let carried = self.assigned(&bs);
                let saved: Vec<u32> = carried.iter().map(|&i| self.env[i].1).collect();
                let rt = self.branch(Some(then), &carried, &saved);
                let re = self.branch(els.as_ref(), &carried, &saved);
                self.structured(Op::If, c, &[rt, re], &carried);
            }
            Stmt::While { cond, body } => {
                let carried = self.assigned(&[body]);
                let n = carried.len() as u32;
                let inits: Vec<u32> = carried.iter().map(|&i| self.env[i].1).collect();
                let parent = self.cur;
                self.cur = self.region(n);
                let args = self.regs[self.cur].args;
                for (k, &c) in carried.iter().enumerate() {
                    self.env[c].1 = args + k as u32;
                }
                let cv = self.expr(cond);
                self.emit(Inst { op: Op::Cond, a: cv, ..Inst::new(Op::Cond) });
                let rc = self.rfirst + self.cur as u32;
                self.cur = self.region(n);
                let args: Vec<u32> = (0..n).map(|k| self.regs[self.cur].args + k).collect();
                let rb = self.branch_with_args(body, &carried, &args);
                self.cur = parent;
                let head: Vec<u32> = [rc, rb].into_iter().chain(inits).collect();
                self.structured(Op::While, 0, &head, &carried);
            }
            Stmt::Return { value } => {
                let v = match value {
                    Some(e) => self.expr(e),
                    None => self.konst(0),
                };
                self.emit(Inst { op: Op::Return, n: 1, a: v, ..Inst::new(Op::Return) });
            }
            Stmt::Match { scrutinee, arms, .. } => {
                let x = self.expr(scrutinee);
                let carried = self.assigned(&arms.iter().map(|a| &a.body).collect::<Vec<_>>());
                let saved: Vec<u32> = carried.iter().map(|&i| self.env[i].1).collect();
                let (mut cases, mut regs, mut default) = (Vec::new(), Vec::new(), 0);
                for arm in arms {
                    let r = self.branch(Some(&arm.body), &carried, &saved);
                    if arm.pats.iter().any(|p| matches!(p, Pat::Wild)) {
                        default = r; // os literais deste braço caem no default de qualquer jeito
                        continue;
                    }
                    for p in &arm.pats {
                        if let Pat::Lit(v) = p {
                            cases.push((*v, regs.len() as u32));
                        }
                    }
                    regs.push(r);
                }
                cases.sort_by_key(|c| c.0);
                let mut head = vec![cases.len() as u32, regs.len() as u32, default];
                for (v, arm) in cases {
                    head.extend([v as u64 as u32, (v as u64 >> 32) as u32, arm]);
                }
                head.extend(regs);
                self.structured(Op::Switch, x, &head, &carried);
            }
            Stmt::Expr(Expr::Call(name, args, ..)) if name == "exit" => {
                let v = self.expr(&args[0]);
                self.emit(Inst { op: Op::Exit, a: v, ..Inst::new(Op::Exit) });
            }
            Stmt::Expr(e) => {
                self.expr(e);
            }
        }
    }

    /// Corpo de laço: a região corrente já existe com argumentos `args`.
    fn branch_with_args(&mut self, body: &'p Block, carried: &[usize], args: &[u32]) -> u32 {
        for (&c, &v) in carried.iter().zip(args) {
            self.env[c].1 = v;
        }
        self.stmts(&body.stmts);
        if !self.regs[self.cur].done {
            let xs: Vec<u32> = carried.iter().map(|&c| self.env[c].1).collect();
            self.yield_(&xs);
        }
        self.rfirst + self.cur as u32
    }

    fn expr(&mut self, e: &'p Expr) -> u32 {
        match e {
            Expr::Num(v) => self.konst(*v),
            Expr::Var(name, ..) => match self.lookup(name) {
                Some(i) => self.env[i].1,
                None => {
                    let p = self.val(Op::Global, 0, self.statics[name.as_str()].0, 0);
                    self.val(Op::Load, 0, p, 0)
                }
            },
            Expr::Index(name, i, ..) => {
                let p = self.element(name, i);
                self.val(Op::Load, 0, p, 0)
            }
            Expr::Unary(op, x) => {
                let x = self.expr(x);
                let k = match op {
                    UnOp::Neg => UnK::Neg,
                    UnOp::Not => UnK::Not,
                };
                self.val(Op::Un, k as u8, x, 0)
            }
            Expr::Binary(op @ (BinOp::And | BinOp::Or), a, b) => {
                // a && b = if a { b ≠ 0 } else { 0 };  a || b = if a { 1 } else { b ≠ 0 }
                let c = self.expr(a);
                let parent = self.cur;
                let arm = |l: &mut Self, eval: bool| {
                    l.cur = l.region(0);
                    let v = if eval {
                        let x = l.expr(b);
                        let z = l.konst(0);
                        l.bin(BinK::Ne, x, z)
                    } else {
                        l.konst(if *op == BinOp::And { 0 } else { 1 })
                    };
                    l.yield_(&[v]);
                    let r = l.rfirst + l.cur as u32;
                    l.cur = parent;
                    r
                };
                let and = *op == BinOp::And;
                let rt = arm(self, and);
                let re = arm(self, !and);
                let b = self.list(&[rt, re, Ty::Word as u32]);
                let res = self.vals(1);
                self.emit(Inst { op: Op::If, k: 0, n: 1, res, a: c, b });
                res
            }
            Expr::Binary(op, a, b) => {
                let (a, b) = (self.expr(a), self.expr(b));
                let k = match op {
                    BinOp::Add => BinK::Add,
                    BinOp::Sub => BinK::Sub,
                    BinOp::Mul => BinK::Mul,
                    BinOp::Div => BinK::Div,
                    BinOp::Rem => BinK::Rem,
                    BinOp::Eq => BinK::Eq,
                    BinOp::Ne => BinK::Ne,
                    BinOp::Lt => BinK::Lt,
                    BinOp::Le => BinK::Le,
                    BinOp::Gt => BinK::Gt,
                    BinOp::Ge => BinK::Ge,
                    BinOp::And | BinOp::Or => unreachable!(),
                };
                self.bin(k, a, b)
            }
            Expr::Call(name, args, ..) => {
                // o CSR é parte da instrução (literal), não um operando
                let (csr, args) = match (name.starts_with("csr_"), &args[..]) {
                    (true, [Expr::Num(c), rest @ ..]) => (*c as u32, rest),
                    _ => (0, &args[..]),
                };
                // argumentos da esquerda para a direita, como em Rust
                let xs: Vec<u32> = args.iter().map(|a| self.expr(a)).collect();
                let intr = match name.as_str() {
                    "sign" => return self.val(Op::Un, UnK::Sgn as u8, xs[0], 0),
                    "shift" => return self.bin(BinK::Sh, xs[0], xs[1]),
                    "csr_read" => IntrK::CsrRead,
                    "csr_write" => IntrK::CsrWrite,
                    "csr_swap" => IntrK::CsrSwap,
                    "wfi" => IntrK::Wfi,
                    "tlbi" => IntrK::Tlbi,
                    "phys_read" => IntrK::PhysRead,
                    "phys_write" => IntrK::PhysWrite,
                    "putc" => IntrK::Putc,
                    "putdec" => IntrK::Putdec,
                    "yield" => IntrK::Yield,
                    "time" => IntrK::Time,
                    "call" => IntrK::Call,
                    "recv" => IntrK::Recv,
                    "recv_val" => IntrK::RecvVal,
                    _ => {
                        let (a, b) = (self.fids[name.as_str()], self.list(&xs));
                        let res = self.vals(1);
                        self.emit(Inst { op: Op::Call, k: 0, n: xs.len() as u16, res, a, b });
                        return res;
                    }
                };
                let b = self.list(&xs);
                let n = xs.len() as u16;
                if super::ir::INTRS[intr as usize].2 {
                    let res = self.vals(1);
                    self.emit(Inst { op: Op::Intr, k: intr as u8, n, res, a: csr, b });
                    res
                } else {
                    // sem resultado (putc, csr_write, ...): como expressão valem 0
                    self.emit(Inst { op: Op::Intr, k: intr as u8, n, res: 0, a: csr, b });
                    self.konst(0)
                }
            }
        }
    }
}
