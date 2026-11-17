//! TENERURA — TIR: representação intermediária estruturada do T27R.
//!
//! Especificação completa em `docs/13-tir.md`. Resumo do contrato:
//!
//! - **Controle estruturado.** Uma função é uma árvore de regiões; não há
//!   grafo de blocos nem PHI. `if`, `switch` e `while` contêm regiões e
//!   devolvem resultados; valores que mudam num ramo saem por `yield`
//!   (argumentos de bloco ao contrário: o pai recebe resultados, a região
//!   de laço recebe argumentos).
//! - **SSA lexical.** Um valor é visível do ponto em que é definido até o
//!   fim da região que o define, inclusive nas regiões aninhadas. Dominância
//!   é escopo: a validação é uma passada só, sem árvore de dominadores.
//! - **Dois tipos.** `word` (27 trits) e `ptr` (opaco, com proveniência).
//!   Não existe conversão entre eles: a proveniência de todo ponteiro é a
//!   de um `slot`, rastreável pelo fluxo de dados.
//! - **Aritmética total.** `wrap` e divisão centrada da ALU auditada;
//!   divisão por zero dá 0. O único comportamento indefinido é acesso de
//!   memória fora da alocação de origem (§UB do docs/13).
//! - **Formato binário único.** Cabeçalho fixo, tabelas de registros de
//!   tamanho fixo, little-endian, campos não usados obrigatoriamente zero:
//!   cada módulo válido tem exatamente uma codificação.

use crate::word::M27;

pub const MAGIC: [u8; 4] = *b"TIR\0";
pub const VERSION: u32 = 1;
/// magic + versão + 6 contagens/índices, tudo u32
pub const HEADER_BYTES: usize = 32;
/// profundidade máxima de regiões aninhadas (limita a recursão da validação)
pub const MAX_DEPTH: u32 = 1000;
/// maior `slot` aceito, em palavras
pub const SLOT_MAX: u32 = 3u32.pow(9);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ty {
    Word = 0,
    Ptr = 1,
}

impl Ty {
    pub fn from_u32(t: u32) -> Option<Ty> {
        match t {
            0 => Some(Ty::Word),
            1 => Some(Ty::Ptr),
            _ => None,
        }
    }
    fn name(self) -> &'static str {
        match self {
            Ty::Word => "word",
            Ty::Ptr => "ptr",
        }
    }
}

/// Opcodes. Campos de `Inst` por opcode (o que não aparece é zero):
///
/// | op | k | n | res | a | b |
/// |---|---|---|---|---|---|
/// | Const  | | | v | lo32 | hi32 (i64 ≤ M27) |
/// | Bin    | BinK | | v | x | y |
/// | Un     | UnK | | v | x | |
/// | Call   | | nargs | v₀ (se a callee retorna) | func | extra: args |
/// | Intr   | IntrK | nargs | v (se tem resultado) | | extra: args |
/// | If     | | nres | v₀ | cond | extra: [then, else, ty×nres] |
/// | Switch | | nres | v₀ | x | extra: [ncases, narms, default, (lo,hi,arm)×ncases, região×narms, ty×nres] |
/// | While  | | nres | v₀ | | extra: [cond, corpo, init×nres, ty×nres] |
/// | Slot   | | | p | palavras | |
/// | PtrAdd | | | p | ptr | word |
/// | Load   | | | v | ptr | |
/// | Store  | | | | ptr | word |
/// | Yield  | | nvals | | | extra: vals |
/// | Cond   | | | | cond | |
/// | Return | | 0/1 | | val | |
/// | Exit   | | | | código | |
///
/// Listas vazias têm deslocamento 0.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Const = 1,
    Bin,
    Un,
    Call,
    Intr,
    If,
    Switch,
    While,
    Slot,
    PtrAdd,
    Load,
    Store,
    Yield,
    Cond,
    Return,
    Exit,
}

impl Op {
    const ALL: [Op; 16] = [
        Op::Const,
        Op::Bin,
        Op::Un,
        Op::Call,
        Op::Intr,
        Op::If,
        Op::Switch,
        Op::While,
        Op::Slot,
        Op::PtrAdd,
        Op::Load,
        Op::Store,
        Op::Yield,
        Op::Cond,
        Op::Return,
        Op::Exit,
    ];
    pub fn from_u8(b: u8) -> Option<Op> {
        Op::ALL.get((b as usize).wrapping_sub(1)).copied()
    }
    pub fn is_term(self) -> bool {
        matches!(self, Op::Yield | Op::Cond | Op::Return | Op::Exit)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinK {
    Add = 0,
    Sub,
    Mul,
    Div,
    Rem,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

pub const BIN_NAMES: [&str; 11] = ["add", "sub", "mul", "div", "rem", "eq", "ne", "lt", "le", "gt", "ge"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnK {
    Neg = 0,
    /// x == 0 ? 1 : 0
    Not,
    /// −1 / 0 / +1
    Sgn,
}

pub const UN_NAMES: [&str; 3] = ["neg", "not", "sgn"];

/// Intrínsecas: efeitos explícitos, aridade e resultado fixos.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntrK {
    Putc = 0,
    Putdec,
    Yield,
    Time,
    Call,
    Recv,
    RecvVal,
}

/// (nome, aridade, tem resultado)
pub const INTRS: [(&str, u16, bool); 7] = [
    ("putc", 1, false),
    ("putdec", 1, false),
    ("yield", 0, true),
    ("time", 0, true),
    ("call", 3, true),
    ("recv", 2, true),
    ("recv_val", 2, true),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Inst {
    pub op: Op,
    pub k: u8,
    pub n: u16,
    pub res: u32,
    pub a: u32,
    pub b: u32,
}

impl Inst {
    pub fn new(op: Op) -> Inst {
        Inst { op, k: 0, n: 0, res: 0, a: 0, b: 0 }
    }
    pub fn imm(&self) -> i64 {
        ((self.b as u64) << 32 | self.a as u64) as i64
    }
}

/// Região: argumentos são os valores `args..args+nargs`; instruções são
/// `first..first+len`, a última é o terminador.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Region {
    pub args: u32,
    pub nargs: u32,
    pub first: u32,
    pub len: u32,
}

/// Entrada da tabela de símbolos. Regiões e instruções de cada função são
/// faixas contíguas; a raiz é `rfirst`. Valores são numerados por função.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Func {
    pub name: u32,
    pub name_len: u32,
    /// extra: [nparams, nres, ty×nparams, ty×nres]
    pub sig: u32,
    pub rfirst: u32,
    pub rcount: u32,
    pub ifirst: u32,
    pub icount: u32,
    pub nvals: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Module {
    pub funcs: Vec<Func>,
    pub regions: Vec<Region>,
    pub insts: Vec<Inst>,
    pub extra: Vec<u32>,
    pub strings: Vec<u8>,
    pub entry: u32,
}

impl Module {
    /// Só para módulos validados (ou construídos pelo `lower`).
    pub fn name(&self, f: usize) -> &str {
        let f = &self.funcs[f];
        std::str::from_utf8(&self.strings[f.name as usize..(f.name + f.name_len) as usize]).unwrap_or("?")
    }

    /// (tipos dos parâmetros, tipos dos resultados) — módulo validado.
    pub fn sig(&self, f: usize) -> (Vec<Ty>, Vec<Ty>) {
        let s = self.funcs[f].sig as usize;
        let (np, nr) = (self.extra[s] as usize, self.extra[s + 1] as usize);
        let t = |i: usize| Ty::from_u32(self.extra[s + 2 + i]).unwrap();
        ((0..np).map(t).collect(), (np..np + nr).map(t).collect())
    }

    pub fn list(&self, off: u32, n: usize) -> &[u32] {
        &self.extra[off as usize..off as usize + n]
    }

    // ── formato binário ─────────────────────────────────────────────────

    pub fn encode(&self) -> Vec<u8> {
        let pad = (4 - self.strings.len() % 4) % 4;
        let len = HEADER_BYTES
            + 32 * self.funcs.len()
            + 16 * (self.regions.len() + self.insts.len())
            + 4 * self.extra.len()
            + self.strings.len()
            + pad;
        let mut o = Vec::with_capacity(len);
        let w = |o: &mut Vec<u8>, x: u32| o.extend_from_slice(&x.to_le_bytes());
        o.extend_from_slice(&MAGIC);
        for x in [
            VERSION,
            self.funcs.len() as u32,
            self.regions.len() as u32,
            self.insts.len() as u32,
            self.extra.len() as u32,
            self.strings.len() as u32,
            self.entry,
        ] {
            w(&mut o, x);
        }
        for f in &self.funcs {
            for x in [f.name, f.name_len, f.sig, f.rfirst, f.rcount, f.ifirst, f.icount, f.nvals] {
                w(&mut o, x);
            }
        }
        for r in &self.regions {
            for x in [r.args, r.nargs, r.first, r.len] {
                w(&mut o, x);
            }
        }
        for i in &self.insts {
            for x in [i.op as u32 | (i.k as u32) << 8 | (i.n as u32) << 16, i.res, i.a, i.b] {
                w(&mut o, x);
            }
        }
        for &x in &self.extra {
            w(&mut o, x);
        }
        o.extend_from_slice(&self.strings);
        o.resize(len, 0);
        o
    }

    /// Decodifica sem interpretar: confere tamanhos, opcodes e preenchimento
    /// e copia as tabelas. A semântica é conferida por `validate`.
    pub fn decode(b: &[u8]) -> Result<Module, String> {
        if b.len() < HEADER_BYTES || b[..4] != MAGIC {
            return Err("não é TIR (magic)".into());
        }
        let u = |i: usize| u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
        if u(4) != VERSION {
            return Err(format!("versão {} não suportada", u(4)));
        }
        let (nf, nr, ni, nx, ns) = (u(8) as u64, u(12) as u64, u(16) as u64, u(20) as u64, u(24) as u64);
        let want = HEADER_BYTES as u64 + 32 * nf + 16 * (nr + ni) + 4 * nx + ns.div_ceil(4) * 4;
        if want != b.len() as u64 {
            return Err(format!("tamanho {} ≠ {} declarado pelo cabeçalho", b.len(), want));
        }
        // registros de tamanho fixo: o campo j do registro i está em base + i·tam + 4j
        let fs = HEADER_BYTES;
        let rs = fs + 32 * nf as usize;
        let is = rs + 16 * nr as usize;
        let xs = is + 16 * ni as usize;
        let ss = xs + 4 * nx as usize;
        let funcs = (0..nf as usize).map(|i| {
            let f = |j: usize| u(fs + 32 * i + 4 * j);
            Func { name: f(0), name_len: f(1), sig: f(2), rfirst: f(3), rcount: f(4), ifirst: f(5), icount: f(6), nvals: f(7) }
        });
        let regions = (0..nr as usize).map(|i| {
            let f = |j: usize| u(rs + 16 * i + 4 * j);
            Region { args: f(0), nargs: f(1), first: f(2), len: f(3) }
        });
        let insts = (0..ni as usize).map(|i| {
            let f = |j: usize| u(is + 16 * i + 4 * j);
            let w = f(0);
            let op = Op::from_u8(w as u8).ok_or_else(|| format!("instrução {i}: opcode {}", w as u8))?;
            Ok(Inst { op, k: (w >> 8) as u8, n: (w >> 16) as u16, res: f(1), a: f(2), b: f(3) })
        });
        let m = Module {
            funcs: funcs.collect(),
            regions: regions.collect(),
            insts: insts.collect::<Result<_, String>>()?,
            extra: (0..nx as usize).map(|i| u(xs + 4 * i)).collect(),
            strings: b[ss..ss + ns as usize].to_vec(),
            entry: u(28),
        };
        let p = ss;
        if b[p + ns as usize..].iter().any(|&x| x != 0) {
            return Err("preenchimento não nulo".into());
        }
        Ok(m)
    }
}

// ── paralelismo determinístico ─────────────────────────────────────────

pub fn jobs() -> usize {
    std::thread::available_parallelism().map_or(1, |n| n.get())
}

/// `(0..n).map(f)` em até `jobs` threads; o resultado sai em ordem de
/// índice, então nada depende do escalonamento.
pub fn par_map<T: Send>(n: usize, jobs: usize, f: impl Fn(usize) -> T + Sync) -> Vec<T> {
    let jobs = jobs.clamp(1, n.max(1));
    if jobs == 1 {
        return (0..n).map(f).collect();
    }
    let chunk = n.div_ceil(jobs);
    let f = &f;
    std::thread::scope(|s| {
        let hs: Vec<_> = (0..jobs)
            .map(|t| s.spawn(move || (t * chunk..((t + 1) * chunk).min(n)).map(f).collect::<Vec<T>>()))
            .collect();
        hs.into_iter().flat_map(|h| h.join().unwrap()).collect()
    })
}

// ── validação ──────────────────────────────────────────────────────────

/// Validação estrutural completa, linear no tamanho do módulo. Funções são
/// independentes depois da checagem de módulo: `jobs > 1` valida em
/// paralelo e devolve o mesmo erro (o da primeira função inválida).
pub fn validate(m: &Module, jobs: usize) -> Result<(), String> {
    validate_module(m)?;
    par_map(m.funcs.len(), jobs, |f| validate_func(m, f).map_err(|e| format!("fn #{f}: {e}")))
        .into_iter()
        .collect()
}

fn ext(m: &Module, off: u32, n: usize) -> Result<&[u32], String> {
    if n == 0 {
        return if off == 0 { Ok(&[]) } else { Err("lista vazia com deslocamento ≠ 0".into()) };
    }
    m.extra
        .get(off as usize..off as usize + n)
        .ok_or_else(|| format!("extra[{off}..+{n}] fora da tabela"))
}

fn tys(xs: &[u32]) -> Result<Vec<Ty>, String> {
    xs.iter().map(|&t| Ty::from_u32(t).ok_or_else(|| format!("tipo {t}"))).collect()
}

/// Checagem de módulo: assinaturas, nomes únicos, faixas que ladrilham as
/// tabelas. Depois dela cada função pode ser validada sozinha.
pub fn validate_module(m: &Module) -> Result<(), String> {
    if m.entry as usize >= m.funcs.len() {
        return Err("entrada fora da tabela de funções".into());
    }
    let (mut r, mut i) = (0u32, 0u32);
    let mut names = std::collections::HashSet::new();
    for (fi, f) in m.funcs.iter().enumerate() {
        let name = m
            .strings
            .get(f.name as usize..f.name as usize + f.name_len as usize)
            .and_then(|s| std::str::from_utf8(s).ok())
            .ok_or_else(|| format!("fn #{fi}: nome fora da tabela ou não UTF-8"))?;
        if name.is_empty() || !names.insert(name) {
            return Err(format!("fn #{fi}: nome '{name}' vazio ou repetido"));
        }
        let h = ext(m, f.sig, 2).map_err(|e| format!("fn #{fi}: {e}"))?;
        let n = h[0] as usize + h[1] as usize;
        if h[0] > 255 || h[1] > 1 {
            return Err(format!("fn #{fi}: assinatura com {} parâmetros, {} resultados", h[0], h[1]));
        }
        tys(ext(m, f.sig + 2, n).map_err(|e| format!("fn #{fi}: {e}"))?)?;
        if f.rfirst != r || f.ifirst != i || f.rcount == 0 {
            return Err(format!("fn #{fi}: faixas não ladrilham as tabelas"));
        }
        r = r.checked_add(f.rcount).ok_or("faixa de regiões transborda")?;
        i = i.checked_add(f.icount).ok_or("faixa de instruções transborda")?;
    }
    if r as usize != m.regions.len() || i as usize != m.insts.len() {
        return Err("regiões ou instruções sem função".into());
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum Kind<'a> {
    Body,
    Yield(&'a [Ty]),
    Cond,
}

struct V<'m> {
    m: &'m Module,
    f: &'m Func,
    ret: Vec<Ty>,
    /// tipo de cada valor definido (u8::MAX = ainda não)
    ty: Vec<u8>,
    live: Vec<bool>,
    scope: Vec<u32>,
    used: Vec<bool>,
}

/// Valida uma função de um módulo que passou por `validate_module`.
pub fn validate_func(m: &Module, fi: usize) -> Result<(), String> {
    let f = &m.funcs[fi];
    let (params, ret) = m.sig(fi);
    // nvals vem do arquivo: limitá-lo pelo que a função pode definir antes
    // de alocar, senão 4 bytes corrompidos pedem gigabytes
    let rs = &m.regions[f.rfirst as usize..(f.rfirst + f.rcount) as usize];
    let is = &m.insts[f.ifirst as usize..(f.ifirst + f.icount) as usize];
    let most: u64 = rs.iter().map(|r| r.nargs as u64).sum::<u64>() + is.iter().map(|i| (i.n as u64).max(1)).sum::<u64>();
    if f.nvals as u64 > most {
        return Err(format!("nvals = {} além dos {most} definíveis", f.nvals));
    }
    let mut v = V {
        m,
        f,
        ret,
        ty: vec![u8::MAX; f.nvals as usize],
        live: vec![false; f.nvals as usize],
        scope: Vec::new(),
        used: vec![false; f.rcount as usize],
    };
    // as regiões ladrilham a faixa de instruções da função, em ordem
    let mut at = f.ifirst;
    for r in &m.regions[f.rfirst as usize..(f.rfirst + f.rcount) as usize] {
        if r.first != at || r.len == 0 {
            return Err("regiões não ladrilham as instruções da função".into());
        }
        at = at.checked_add(r.len).ok_or("região transborda")?;
    }
    if at != f.ifirst + f.icount {
        return Err("instruções sem região".into());
    }
    v.region(f.rfirst, &params, Kind::Body, 0)?;
    if let Some(r) = v.used.iter().position(|u| !u) {
        return Err(format!("região {} inalcançável", f.rfirst as usize + r));
    }
    if let Some(x) = v.ty.iter().position(|&t| t == u8::MAX) {
        return Err(format!("%{x} nunca definido"));
    }
    Ok(())
}

impl<'m> V<'m> {
    fn def(&mut self, x: u32, t: Ty) -> Result<(), String> {
        match self.ty.get(x as usize) {
            Some(&u8::MAX) => {}
            Some(_) => return Err(format!("%{x} definido duas vezes")),
            None => return Err(format!("%{x} além de nvals")),
        }
        self.ty[x as usize] = t as u8;
        self.live[x as usize] = true;
        self.scope.push(x);
        Ok(())
    }

    fn defs(&mut self, first: u32, ts: &[Ty]) -> Result<(), String> {
        for (i, &t) in ts.iter().enumerate() {
            self.def(first.checked_add(i as u32).ok_or("valor transborda")?, t)?;
        }
        Ok(())
    }

    fn use_(&self, x: u32, t: Ty) -> Result<(), String> {
        if !self.live.get(x as usize).copied().unwrap_or(false) {
            return Err(format!("%{x} usado fora do escopo"));
        }
        if self.ty[x as usize] != t as u8 {
            return Err(format!("%{x} não é {}", t.name()));
        }
        Ok(())
    }

    fn uses(&self, xs: &[u32], ts: &[Ty]) -> Result<(), String> {
        xs.iter().zip(ts).try_for_each(|(&x, &t)| self.use_(x, t))
    }

    fn region(&mut self, r: u32, args: &[Ty], kind: Kind, depth: u32) -> Result<(), String> {
        let local = r.wrapping_sub(self.f.rfirst) as usize;
        if local >= self.used.len() {
            return Err(format!("região {r} fora da função"));
        }
        if std::mem::replace(&mut self.used[local], true) {
            return Err(format!("região {r} usada duas vezes"));
        }
        if depth > MAX_DEPTH {
            return Err("aninhamento além de MAX_DEPTH".into());
        }
        let reg = self.m.regions[r as usize];
        if reg.nargs as usize != args.len() {
            return Err(format!("região {r}: {} argumentos, esperava {}", reg.nargs, args.len()));
        }
        let mark = self.scope.len();
        self.defs(reg.args, args)?;
        let end = reg.first + reg.len;
        for idx in reg.first..end {
            let i = self.m.insts[idx as usize];
            if i.op.is_term() != (idx + 1 == end) {
                return Err(format!("instrução {idx}: terminador fora do fim da região"));
            }
            self.inst(&i, kind, depth).map_err(|e| format!("instrução {idx} ({:?}): {e}", i.op))?;
        }
        for x in self.scope.drain(mark..) {
            self.live[x as usize] = false;
        }
        Ok(())
    }

    fn inst(&mut self, i: &Inst, kind: Kind, depth: u32) -> Result<(), String> {
        let z = |xs: &[u32]| if xs.iter().all(|&x| x == 0) { Ok(()) } else { Err("campo não usado ≠ 0".to_string()) };
        let (k, n) = (i.k as u32, i.n as u32);
        let m = self.m;
        let w = Ty::Word;
        match i.op {
            Op::Const => {
                z(&[k, n])?;
                if !(-M27..=M27).contains(&i.imm()) {
                    return Err(format!("constante {} fora de 27 trits", i.imm()));
                }
                self.def(i.res, w)
            }
            Op::Bin => {
                z(&[n])?;
                if k as usize >= BIN_NAMES.len() {
                    return Err(format!("binária {k}"));
                }
                self.uses(&[i.a, i.b], &[w, w])?;
                self.def(i.res, w)
            }
            Op::Un => {
                z(&[n, i.b])?;
                if k as usize >= UN_NAMES.len() {
                    return Err(format!("unária {k}"));
                }
                self.use_(i.a, w)?;
                self.def(i.res, w)
            }
            Op::Call => {
                z(&[k])?;
                if i.a as usize >= m.funcs.len() {
                    return Err(format!("função #{} não existe", i.a));
                }
                let (ps, rs) = m.sig(i.a as usize);
                if ps.len() != n as usize {
                    return Err(format!("{} argumentos, a função espera {}", n, ps.len()));
                }
                self.uses(ext(m, i.b, ps.len())?, &ps)?;
                if rs.is_empty() {
                    z(&[i.res])
                } else {
                    self.defs(i.res, &rs)
                }
            }
            Op::Intr => {
                z(&[i.a])?;
                let &(_, arity, has_res) = INTRS.get(k as usize).ok_or(format!("intrínseca {k}"))?;
                if i.n != arity {
                    return Err(format!("{} argumentos, a intrínseca espera {arity}", i.n));
                }
                self.uses(ext(m, i.b, n as usize)?, &[w; 3])?;
                if has_res {
                    self.def(i.res, w)
                } else {
                    z(&[i.res])
                }
            }
            Op::If => {
                z(&[k])?;
                self.use_(i.a, w)?;
                let x = ext(m, i.b, 2 + n as usize)?;
                let ts = tys(&x[2..])?;
                self.region(x[0], &[], Kind::Yield(&ts), depth + 1)?;
                self.region(x[1], &[], Kind::Yield(&ts), depth + 1)?;
                self.defs(i.res, &ts)
            }
            Op::Switch => {
                z(&[k])?;
                self.use_(i.a, w)?;
                let h = ext(m, i.b, 3)?;
                let (nc, na) = (h[0] as usize, h[1] as usize);
                let x = ext(m, i.b, 3 + 3 * nc + na + n as usize)?;
                let ts = tys(&x[3 + 3 * nc + na..])?;
                let mut hit = vec![false; na];
                let mut prev: Option<i64> = None;
                for c in x[3..3 + 3 * nc].chunks(3) {
                    let lit = ((c[1] as u64) << 32 | c[0] as u64) as i64;
                    if !(-M27..=M27).contains(&lit) || prev.is_some_and(|p| p >= lit) {
                        return Err(format!("caso {lit}: fora de 27 trits ou fora de ordem crescente"));
                    }
                    prev = Some(lit);
                    *hit.get_mut(c[2] as usize).ok_or(format!("caso {lit}: braço {} não existe", c[2]))? = true;
                }
                if hit.contains(&false) {
                    return Err("braço sem caso".into());
                }
                self.region(x[2], &[], Kind::Yield(&ts), depth + 1)?;
                for &r in &x[3 + 3 * nc..3 + 3 * nc + na] {
                    self.region(r, &[], Kind::Yield(&ts), depth + 1)?;
                }
                self.defs(i.res, &ts)
            }
            Op::While => {
                z(&[k, i.a])?;
                let x = ext(m, i.b, 2 + 2 * n as usize)?;
                let ts = tys(&x[2 + n as usize..])?;
                self.uses(&x[2..2 + n as usize], &ts)?;
                self.region(x[0], &ts, Kind::Cond, depth + 1)?;
                self.region(x[1], &ts, Kind::Yield(&ts), depth + 1)?;
                self.defs(i.res, &ts)
            }
            Op::Slot => {
                z(&[k, n, i.b])?;
                if i.a == 0 || i.a > SLOT_MAX {
                    return Err(format!("slot de {} palavras", i.a));
                }
                self.def(i.res, Ty::Ptr)
            }
            Op::PtrAdd => {
                z(&[k, n])?;
                self.uses(&[i.a, i.b], &[Ty::Ptr, w])?;
                self.def(i.res, Ty::Ptr)
            }
            Op::Load => {
                z(&[k, n, i.b])?;
                self.use_(i.a, Ty::Ptr)?;
                self.def(i.res, w)
            }
            Op::Store => {
                z(&[k, n, i.res])?;
                self.uses(&[i.a, i.b], &[Ty::Ptr, w])
            }
            Op::Yield => {
                z(&[k, i.res, i.a])?;
                let Kind::Yield(ts) = kind else { return Err("yield fora de ramo ou corpo de laço".into()) };
                if n as usize != ts.len() {
                    return Err(format!("yield de {n} valores, esperava {}", ts.len()));
                }
                self.uses(ext(m, i.b, n as usize)?, ts)
            }
            Op::Cond => {
                z(&[k, n, i.res, i.b])?;
                if !matches!(kind, Kind::Cond) {
                    return Err("cond fora da região de condição".into());
                }
                self.use_(i.a, w)
            }
            Op::Return => {
                z(&[k, i.res, i.b])?;
                if matches!(kind, Kind::Cond) {
                    return Err("return na região de condição".into());
                }
                if n as usize != self.ret.len() {
                    return Err(format!("return de {n} valores, a função devolve {}", self.ret.len()));
                }
                match self.ret.first() {
                    Some(&t) => self.use_(i.a, t),
                    None => z(&[i.a]),
                }
            }
            Op::Exit => {
                z(&[k, n, i.res, i.b])?;
                if matches!(kind, Kind::Cond) {
                    return Err("exit na região de condição".into());
                }
                self.use_(i.a, w)
            }
        }
    }
}

// ── texto (só para leitura humana; o formato é o binário) ──────────────

pub fn print(m: &Module) -> String {
    let mut o = String::new();
    for fi in 0..m.funcs.len() {
        let f = m.funcs[fi];
        let (ps, rs) = m.sig(fi);
        let root = m.regions[f.rfirst as usize];
        let params: Vec<String> = ps.iter().enumerate().map(|(i, t)| format!("%{}: {}", root.args + i as u32, t.name())).collect();
        let rets: Vec<&str> = rs.iter().map(|t| t.name()).collect();
        o += &format!("fn @{}({}) -> ({}) {{\n", m.name(fi), params.join(", "), rets.join(", "));
        body(m, f.rfirst, 1, &mut o);
        o += "}\n";
    }
    o
}

fn vals(xs: impl IntoIterator<Item = u32>) -> String {
    xs.into_iter().map(|x| format!("%{x}")).collect::<Vec<_>>().join(", ")
}

fn results(i: &Inst) -> String {
    if i.n == 0 {
        String::new()
    } else {
        format!("{} = ", vals(i.res..i.res + i.n as u32))
    }
}

fn args(r: &Region) -> String {
    vals(r.args..r.args + r.nargs)
}

fn body(m: &Module, r: u32, d: usize, o: &mut String) {
    let reg = m.regions[r as usize];
    let pad = "  ".repeat(d);
    for idx in reg.first..reg.first + reg.len {
        let i = m.insts[idx as usize];
        let n = i.n as usize;
        let line = match i.op {
            Op::Const => format!("%{} = const {}", i.res, i.imm()),
            Op::Bin => format!("%{} = {} %{}, %{}", i.res, BIN_NAMES[i.k as usize], i.a, i.b),
            Op::Un => format!("%{} = {} %{}", i.res, UN_NAMES[i.k as usize], i.a),
            Op::Call => format!("%{} = call @{}({})", i.res, m.name(i.a as usize), vals(m.list(i.b, n).iter().copied())),
            Op::Intr => {
                let (name, _, has) = INTRS[i.k as usize];
                let lhs = if has { format!("%{} = ", i.res) } else { String::new() };
                format!("{lhs}{name}!({})", vals(m.list(i.b, n).iter().copied()))
            }
            Op::Slot => format!("%{} = slot {}", i.res, i.a),
            Op::PtrAdd => format!("%{} = ptradd %{}, %{}", i.res, i.a, i.b),
            Op::Load => format!("%{} = load %{}", i.res, i.a),
            Op::Store => format!("store %{}, %{}", i.a, i.b),
            Op::Yield if n == 0 => "yield".into(),
            Op::Yield => format!("yield {}", vals(m.list(i.b, n).iter().copied())),
            Op::Cond => format!("cond %{}", i.a),
            Op::Return if n == 0 => "return".into(),
            Op::Return => format!("return %{}", i.a),
            Op::Exit => format!("exit %{}", i.a),
            Op::If => {
                let x = m.list(i.b, 2);
                *o += &format!("{pad}{}if %{} {{\n", results(&i), i.a);
                body(m, x[0], d + 1, o);
                *o += &format!("{pad}}} else {{\n");
                body(m, x[1], d + 1, o);
                *o += &format!("{pad}}}\n");
                continue;
            }
            Op::Switch => {
                let h = m.list(i.b, 3);
                let (nc, na) = (h[0] as usize, h[1] as usize);
                let x = m.list(i.b, 3 + 3 * nc + na);
                *o += &format!("{pad}{}switch %{} {{\n", results(&i), i.a);
                for (k, &ar) in x[3 + 3 * nc..].iter().enumerate() {
                    let lits: Vec<String> = x[3..3 + 3 * nc]
                        .chunks(3)
                        .filter(|c| c[2] as usize == k)
                        .map(|c| (((c[1] as u64) << 32 | c[0] as u64) as i64).to_string())
                        .collect();
                    *o += &format!("{pad}case {}:\n", lits.join(" | "));
                    body(m, ar, d + 1, o);
                }
                *o += &format!("{pad}default:\n");
                body(m, x[2], d + 1, o);
                *o += &format!("{pad}}}\n");
                continue;
            }
            Op::While => {
                let x = m.list(i.b, 2 + n);
                let (c, b) = (m.regions[x[0] as usize], m.regions[x[1] as usize]);
                *o += &format!(
                    "{pad}{}while ({}) cond({}) {{\n",
                    results(&i),
                    vals(x[2..2 + n].iter().copied()),
                    args(&c)
                );
                body(m, x[0], d + 1, o);
                *o += &format!("{pad}}} body({}) {{\n", args(&b));
                body(m, x[1], d + 1, o);
                *o += &format!("{pad}}}\n");
                continue;
            }
        };
        *o += &format!("{pad}{line}\n");
    }
}
