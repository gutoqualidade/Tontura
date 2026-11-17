//! TENERURA — geração de assembly T27 a partir do subconjunto.
//!
//! Estratégia deliberadamente simples (máquina de pilha): toda expressão
//! deixa o resultado em `a1`; binárias empilham o operando esquerdo e
//! desempilham em `t1`. Não há alocação de registradores — o objetivo é
//! correção e rastreabilidade, não código ótimo.
//!
//! Quadro de cada função:
//!
//! ```text
//! fn_x:
//!     push lr            ; endereço de retorno
//!     push s11           ; quadro anterior
//!     mov  s11, sp       ; s11 = base do quadro
//!     addi sp, sp, -N    ; N locais (let), em s11-1 .. s11-N
//!     ... corpo ...
//!     li a1, 0           ; retorno implícito
//! .Lret_x:
//!     mov sp, s11
//!     pop s11
//!     pop lr
//!     ret
//! ```

use std::collections::HashMap;

use super::parse::{Arm, BinOp, Block, Expr, Fn, Pat, Program, Stmt, UnOp};
use crate::isa::IMM16_MAX;

pub(super) const HEADER: &str = "\
; ────────────────────────────────────────────────────────────
; gerado por t27r (subconjunto de Rust para T27) — não editar
; ────────────────────────────────────────────────────────────
    .include \"../lib/abi.inc\"
    .org USER_TEXT
_start:
    call fn_main
    mov a2, a1
    j exit
";

pub(super) const FOOTER: &str = "\
    .text
    .include \"../lib/ulib.inc\"
";

pub struct Gen {
    out: String,
    nlabel: usize,
    /// locais já alocados na função corrente
    locals: usize,
    scopes: Vec<HashMap<String, usize>>,
    ret: String,
}

impl Gen {
    fn new() -> Self {
        Gen { out: String::new(), nlabel: 0, locals: 0, scopes: Vec::new(), ret: String::new() }
    }

    fn line(&mut self, s: &str) {
        self.out.push_str(s);
        self.out.push('\n');
    }

    fn label(&mut self) -> String {
        self.nlabel += 1;
        format!(".L{}", self.nlabel)
    }

    fn declare(&mut self, name: &str, slot: usize) {
        self.scopes.last_mut().unwrap().insert(name.to_string(), slot);
    }

    fn slot(&self, name: &str) -> usize {
        self.scopes
            .iter()
            .rev()
            .find_map(|s| s.get(name).copied())
            .unwrap_or_else(|| panic!("t27r: '{name}' passou pela checagem mas não tem local"))
    }

    /// Carrega um valor imediato em `reg`: `li` quando cabe em 16 trits,
    /// `la` (dois passos) caso contrário.
    fn imm(&mut self, reg: &str, v: i64) {
        if v.abs() <= IMM16_MAX {
            self.line(&format!("    li {reg}, {v}"));
        } else {
            self.line(&format!("    la {reg}, {v}"));
        }
    }

    fn block(&mut self, b: &Block) {
        self.scopes.push(HashMap::new());
        for s in &b.stmts {
            self.stmt(s);
        }
        self.scopes.pop();
    }

    fn stmt(&mut self, s: &Stmt) {
        match s {
            Stmt::Let { name, init, .. } => {
                self.expr(init);
                let slot = self.locals;
                self.locals += 1;
                self.declare(name, slot);
                self.line(&format!("    st a1, {}(s11)", -(slot as i64 + 1)));
            }
            Stmt::Assign { name, value, .. } => {
                self.expr(value);
                let slot = self.slot(name);
                self.line(&format!("    st a1, {}(s11)", -(slot as i64 + 1)));
            }
            Stmt::If { cond, then, els } => {
                let (l_else, l_end) = (self.label(), self.label());
                self.expr(cond);
                self.line(&format!("    bz a1, {l_else}"));
                self.block(then);
                match els {
                    Some(e) => {
                        self.line(&format!("    j {l_end}"));
                        self.line(&format!("{l_else}:"));
                        self.block(e);
                    }
                    None => self.line(&format!("{l_else}:")),
                }
                self.line(&format!("{l_end}:"));
            }
            Stmt::While { cond, body } => {
                let (l_start, l_end) = (self.label(), self.label());
                self.line(&format!("{l_start}:"));
                self.expr(cond);
                self.line(&format!("    bz a1, {l_end}"));
                self.block(body);
                self.line(&format!("    j {l_start}"));
                self.line(&format!("{l_end}:"));
            }
            Stmt::Return { value } => {
                match value {
                    Some(e) => self.expr(e),
                    None => self.imm("a1", 0),
                }
                self.line(&format!("    j {}", self.ret));
            }
            Stmt::Match { scrutinee, arms, .. } => self.match_stmt(scrutinee, arms),
            Stmt::Store { .. } => unreachable!("static: recusado em compile_pilha"),
            Stmt::Expr(e) => {
                self.expr(e);
            }
        }
    }

    fn match_stmt(&mut self, scrutinee: &Expr, arms: &[Arm]) {
        self.expr(scrutinee); // a1 = valor
        let l_end = self.label();
        let mut l_wild: Option<String> = None;
        let mut bodies: Vec<(String, &Block)> = Vec::new();
        for arm in arms {
            if arm.pats.iter().any(|p| matches!(p, Pat::Wild)) {
                let l = self.label();
                l_wild = Some(l.clone());
                bodies.push((l, &arm.body));
                continue;
            }
            let l = self.label();
            for p in &arm.pats {
                if let Pat::Lit(v) = p {
                    self.imm("t1", *v);
                    self.line(&format!("    beq a1, t1, {l}"));
                }
            }
            bodies.push((l, &arm.body));
        }
        // o '_' garante destino; sem ele a checagem já teria recusado
        let fallthrough = l_wild.clone().unwrap_or_else(|| l_end.clone());
        self.line(&format!("    j {fallthrough}"));
        for (l, body) in bodies {
            self.line(&format!("{l}:"));
            self.block(body);
            self.line(&format!("    j {l_end}"));
        }
        self.line(&format!("{l_end}:"));
    }

    fn expr(&mut self, e: &Expr) {
        match e {
            Expr::Num(v) => self.imm("a1", *v),
            Expr::Var(name, _, _) => {
                let slot = self.slot(name);
                self.line(&format!("    ld a1, {}(s11)", -(slot as i64 + 1)));
            }
            Expr::Unary(UnOp::Neg, x) => {
                self.expr(x);
                self.line("    neg a1, a1");
            }
            Expr::Unary(UnOp::Not, x) => {
                // !x = (x == 0)
                self.expr(x);
                let l = self.label();
                self.line("    cmp t1, a1, zr");
                self.imm("a1", 0);
                self.line(&format!("    bnz t1, {l}"));
                self.imm("a1", 1);
                self.line(&format!("{l}:"));
            }
            Expr::Binary(op @ (BinOp::And | BinOp::Or), a, b) => {
                let l_end = self.label();
                let short = self.label();
                self.expr(a);
                match op {
                    BinOp::And => self.line(&format!("    bz a1, {short}")),
                    _ => self.line(&format!("    bnz a1, {short}")),
                }
                self.expr(b);
                match op {
                    BinOp::And => self.line(&format!("    bz a1, {short}")),
                    _ => self.line(&format!("    bnz a1, {short}")),
                }
                self.imm("a1", if *op == BinOp::And { 1 } else { 0 });
                self.line(&format!("    j {l_end}"));
                self.line(&format!("{short}:"));
                self.imm("a1", if *op == BinOp::And { 0 } else { 1 });
                self.line(&format!("{l_end}:"));
            }
            Expr::Binary(op, a, b) => {
                self.expr(a);
                self.line("    push a1");
                self.expr(b);
                self.line("    pop t1");
                match op {
                    BinOp::Add => self.line("    add a1, t1, a1"),
                    BinOp::Sub => self.line("    sub a1, t1, a1"),
                    BinOp::Mul => self.line("    mul a1, t1, a1"),
                    BinOp::Div => self.line("    div a1, t1, a1"),
                    BinOp::Rem => self.line("    rem a1, t1, a1"),
                    _ => {
                        // comparação: cmp devolve o sinal de (t1 − a1)
                        let l = self.label();
                        self.line("    cmp t2, t1, a1");
                        self.imm("a1", 0);
                        let skip = match op {
                            BinOp::Eq => format!("    bnz t2, {l}"),
                            BinOp::Ne => format!("    bz t2, {l}"),
                            BinOp::Lt => format!("    bnn t2, {l}"),
                            BinOp::Le => format!("    bp t2, {l}"),
                            BinOp::Gt => format!("    bnp t2, {l}"),
                            BinOp::Ge => format!("    bn t2, {l}"),
                            _ => unreachable!(),
                        };
                        self.line(&skip);
                        self.imm("a1", 1);
                        self.line(&format!("{l}:"));
                    }
                }
            }
            Expr::Call(name, args, _, _) => self.call(name, args),
            Expr::Index(..) => unreachable!("static: recusado em compile_pilha"),
        }
    }

    fn call(&mut self, name: &str, args: &[Expr]) {
        match name {
            "putc" => {
                self.expr(&args[0]);
                self.line("    mov a2, a1");
                self.line("    call putc");
            }
            "putdec" => {
                self.expr(&args[0]);
                self.line("    mov a2, a1");
                self.line("    call putdec");
                self.imm("a1", 0);
            }
            "yield" => {
                self.line("    li a1, SYS_YIELD");
                self.line("    sys");
            }
            "time" => {
                self.line("    li a1, SYS_TIME");
                self.line("    sys");
            }
            "shift" => {
                self.expr(&args[0]);
                self.line("    push a1");
                self.expr(&args[1]);
                self.line("    pop t1");
                self.line("    sh a1, t1, a1");
            }
            "sign" => {
                self.expr(&args[0]);
                self.line("    sgn a1, a1");
            }
            "call" => {
                // protocolo (op, valor) → valor, enquanto não há structs:
                // avalia de trás para frente para não perder a2 no caminho
                self.expr(&args[2]);
                self.line("    push a1");
                self.expr(&args[1]);
                self.line("    push a1");
                self.expr(&args[0]);
                self.line("    mov a2, a1");
                self.line("    pop a3");
                self.line("    pop a4");
                self.line("    li t1, -1");
                self.line("    li a1, SYS_CALL");
                self.line("    sys");
                let l_ok = self.label();
                let l_end = self.label();
                self.line("    li t2, 1");
                self.line(&format!("    beq a1, t2, {l_ok}"));
                self.line(&format!("    j {l_end}")); // erro/nada: a1 já é o estado
                self.line(&format!("{l_ok}:"));
                self.line("    mov a1, a3");
                self.line(&format!("{l_end}:"));
            }
            "recv" | "recv_val" => {
                // recv(cap, paciência): avalia a paciência primeiro para
                // não perder a2 quando a capacidade for calculada
                self.expr(&args[1]);
                self.line("    push a1");
                self.expr(&args[0]);
                self.line("    mov a2, a1");
                self.line("    pop t1");
                self.line("    li a1, SYS_RECV");
                self.line("    sys");
                if name == "recv_val" {
                    // sem mensagem (0) ou erro (−1) o estado é o resultado;
                    // com mensagem, a primeira palavra recebida é a3
                    let (l_ok, l_end) = (self.label(), self.label());
                    self.line("    li t2, 1");
                    self.line(&format!("    beq a1, t2, {l_ok}"));
                    self.line(&format!("    j {l_end}"));
                    self.line(&format!("{l_ok}:"));
                    self.line("    mov a1, a3");
                    self.line(&format!("{l_end}:"));
                }
            }
            "exit" => {
                self.expr(&args[0]);
                self.line("    mov a2, a1");
                self.line("    j exit");
            }
            _ => {
                for a in args {
                    self.expr(a);
                    self.line("    push a1");
                }
                for i in (0..args.len()).rev() {
                    self.line(&format!("    pop a{}", i + 1));
                }
                self.line(&format!("    call fn_{name}"));
            }
        }
    }

    fn function(&mut self, f: &Fn) {
        self.locals = 0;
        self.scopes.clear();
        self.ret = format!(".Lret_{}", f.name);
        // corpo primeiro: só depois de gerá-lo sabemos quantos locais há
        let mut body = Gen::new();
        body.nlabel = self.nlabel;
        body.ret = self.ret.clone();
        body.locals = 0;
        body.scopes.push(HashMap::new());
        for p in &f.params {
            // parâmetros moram nas posições 0..n-1, gravados pelo chamador
            // em a1..a9 e guardados no prólogo
            let slot = body.locals;
            body.locals += 1;
            body.declare(p, slot);
        }
        for s in &f.body.stmts {
            body.stmt(s);
        }
        let locals = body.locals;
        self.nlabel = body.nlabel;
        self.line(&format!("fn_{}:", f.name));
        self.line("    push lr");
        self.line("    push s11");
        self.line("    mov s11, sp");
        if locals > 0 {
            self.line(&format!("    addi sp, sp, -{locals}"));
        }
        for (i, _) in f.params.iter().enumerate() {
            self.line(&format!("    st a{}, {}(s11)", i + 1, -(i as i64 + 1)));
        }
        self.out.push_str(&body.out);
        self.imm("a1", 0);
        self.line(&format!("{}:", self.ret));
        self.line("    mov sp, s11");
        self.line("    pop s11");
        self.line("    pop lr");
        self.line("    ret");
    }
}

pub fn generate(p: &Program) -> String {
    let mut g = Gen::new();
    g.out.push_str(HEADER);
    for f in &p.fns {
        g.function(f);
    }
    g.out.push_str(FOOTER);
    g.out
}
