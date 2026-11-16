//! TENERURA T27 — montador (docs/07-isa-t27.md §9).
//!
//! Montador de ponto fixo: cada passagem recalcula endereços e valores
//! usando, para símbolos ainda não vistos, a tabela da passagem anterior;
//! repete até a tabela estabilizar. Como toda instrução tem tamanho fixo,
//! a convergência é imediata na prática (2–3 passagens).
//!
//! Sintaxe: `rótulo: mnemônico operandos ; comentário`. Seções `.text`
//! (RX) e `.data` (RW; começa no início da primeira página após o texto,
//! salvo `.org` explícito). Rótulos locais numéricos (`1:`, `1b`, `1f`).
//! Em expressões, `/` e `%` são a divisão CENTRADA da máquina (`Word::divrem`).

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use crate::isa::{self, csr, encode, Fmt, Instr, IsaError, Op};
use crate::machine::{PAGE_HALF, PAGE_SIZE};
use crate::word::{Word, M27, POW3};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Perm {
    /// leitura + execução
    Rx,
    /// leitura + escrita
    Rw,
}

#[derive(Clone, Debug)]
pub struct Segment {
    pub base: i64,
    pub words: Vec<Word>,
    pub perm: Perm,
}

impl Segment {
    /// Endereço um além da última palavra.
    pub fn end(&self) -> i64 {
        self.base + self.words.len() as i64
    }
}

/// Resultado da montagem.
#[derive(Clone, Debug)]
pub struct Program {
    pub segments: Vec<Segment>,
    pub symbols: BTreeMap<String, i64>,
    pub entry: i64,
    /// (endereço, palavra, linha-fonte) de cada palavra emitida
    pub listing: Vec<(i64, Word, String)>,
}

impl Program {
    pub fn sym(&self, name: &str) -> i64 {
        *self
            .symbols
            .get(name)
            .unwrap_or_else(|| panic!("símbolo ausente: {name}"))
    }

    pub fn text(&self) -> &Segment {
        &self.segments[0]
    }

    pub fn data(&self) -> Option<&Segment> {
        self.segments.get(1)
    }
}

#[derive(Clone, Debug)]
pub struct AsmError {
    pub file: String,
    pub line: usize,
    pub msg: String,
}

impl std::fmt::Display for AsmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}: {}", self.file, self.line, self.msg)
    }
}

impl std::error::Error for AsmError {}

/// Início (menor endereço) da primeira página que começa em `addr` ou depois.
pub fn page_start_at_or_after(addr: i64) -> i64 {
    let p = (addr + PAGE_HALF + PAGE_SIZE - 1).div_euclid(PAGE_SIZE);
    p * PAGE_SIZE - PAGE_HALF
}

// ───────────────────────── pré-processamento ─────────────────────────

#[derive(Clone, Debug)]
struct Line {
    file: String,
    line: usize,
    labels: Vec<String>,
    mnemonic: String,
    operands: Vec<String>,
    text: String,
}

struct Macro {
    params: Vec<String>,
    body: Vec<(String, usize, String)>,
}

/// Remove o comentário (`;`) respeitando aspas.
fn strip_comment(s: &str) -> &str {
    let mut in_str = false;
    let mut in_chr = false;
    let mut prev_bs = false;
    for (i, ch) in s.char_indices() {
        if prev_bs {
            prev_bs = false;
            continue;
        }
        match ch {
            '\\' if in_str || in_chr => prev_bs = true,
            '"' if !in_chr => in_str = !in_str,
            '\'' if !in_str => in_chr = !in_chr,
            ';' if !in_str && !in_chr => return &s[..i],
            _ => {}
        }
    }
    s
}

/// Divide operandos nas vírgulas de nível zero (fora de parênteses/aspas).
fn split_operands(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut in_str = false;
    let mut in_chr = false;
    let mut prev_bs = false;
    let mut cur = String::new();
    for ch in s.chars() {
        if prev_bs {
            prev_bs = false;
            cur.push(ch);
            continue;
        }
        match ch {
            '\\' if in_str || in_chr => {
                prev_bs = true;
                cur.push(ch);
            }
            '"' if !in_chr => {
                in_str = !in_str;
                cur.push(ch);
            }
            '\'' if !in_str => {
                in_chr = !in_chr;
                cur.push(ch);
            }
            '(' if !in_str && !in_chr => {
                depth += 1;
                cur.push(ch);
            }
            ')' if !in_str && !in_chr => {
                depth -= 1;
                cur.push(ch);
            }
            ',' if depth == 0 && !in_str && !in_chr => {
                out.push(cur.trim().to_string());
                cur.clear();
            }
            _ => cur.push(ch),
        }
    }
    if !cur.trim().is_empty() || !out.is_empty() {
        out.push(cur.trim().to_string());
    }
    out
}

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_' || c == '.' || c == '$'
}

fn is_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '$'
}

fn is_ident(s: &str) -> bool {
    let mut it = s.chars();
    matches!(it.next(), Some(c) if is_ident_start(c)) && it.all(is_ident_char)
}

struct Pre {
    lines: Vec<Line>,
    macros: HashMap<String, Macro>,
    base_dir: Option<PathBuf>,
    depth: usize,
}

impl Pre {
    fn err(file: &str, line: usize, msg: impl Into<String>) -> AsmError {
        AsmError { file: file.to_string(), line, msg: msg.into() }
    }

    /// Separa rótulos, mnemônico e operandos de uma linha já sem comentário.
    fn parse_line(file: &str, line_no: usize, raw: &str) -> Result<Option<Line>, AsmError> {
        let mut rest = raw.trim();
        let mut labels = Vec::new();
        loop {
            // rótulo: identificador ou número local seguido de ':'
            let end = rest.find(|c: char| !(is_ident_char(c))).unwrap_or(rest.len());
            if end > 0 && rest[end..].starts_with(':') {
                let name = &rest[..end];
                if !(is_ident(name) || name.chars().all(|c| c.is_ascii_digit())) {
                    return Err(Self::err(file, line_no, format!("rótulo inválido: {name}")));
                }
                labels.push(name.to_string());
                rest = rest[end + 1..].trim_start();
            } else {
                break;
            }
        }
        if rest.is_empty() && labels.is_empty() {
            return Ok(None);
        }
        let (mn, ops) = match rest.find(char::is_whitespace) {
            Some(i) => (&rest[..i], rest[i..].trim()),
            None => (rest, ""),
        };
        // forma `nome = expr`
        if let Some(eq) = ops.strip_prefix('=') {
            if is_ident(mn) {
                return Ok(Some(Line {
                    file: file.to_string(),
                    line: line_no,
                    labels,
                    mnemonic: ".equ".to_string(),
                    operands: vec![mn.to_string(), eq.trim().to_string()],
                    text: raw.trim().to_string(),
                }));
            }
        }
        Ok(Some(Line {
            file: file.to_string(),
            line: line_no,
            labels,
            mnemonic: mn.to_string(),
            operands: split_operands(ops),
            text: raw.trim().to_string(),
        }))
    }

    fn feed(&mut self, file: &str, src: &str) -> Result<(), AsmError> {
        self.depth += 1;
        if self.depth > 16 {
            return Err(Self::err(file, 0, "inclusão/macro aninhada demais"));
        }
        let mut it = src.lines().enumerate();
        while let Some((i, raw)) = it.next() {
            let line_no = i + 1;
            let Some(line) = Self::parse_line(file, line_no, strip_comment(raw))? else {
                continue;
            };
            match line.mnemonic.as_str() {
                ".include" => {
                    let name = line
                        .operands
                        .first()
                        .and_then(|s| s.strip_prefix('"'))
                        .and_then(|s| s.strip_suffix('"'))
                        .ok_or_else(|| Self::err(file, line_no, ".include \"arquivo\""))?;
                    let dir = self
                        .base_dir
                        .clone()
                        .ok_or_else(|| Self::err(file, line_no, ".include sem diretório base"))?;
                    let path = dir.join(name);
                    let text = std::fs::read_to_string(&path).map_err(|e| {
                        Self::err(file, line_no, format!("não li {}: {e}", path.display()))
                    })?;
                    self.feed(name, &text)?;
                }
                ".macro" => {
                    let mut ops = line.operands.clone();
                    // `.macro NOME p1, p2` — o nome vem colado ao 1º parâmetro
                    let first = ops.first().cloned().unwrap_or_default();
                    let mut parts = first.split_whitespace();
                    let name = parts
                        .next()
                        .ok_or_else(|| Self::err(file, line_no, ".macro sem nome"))?
                        .to_string();
                    let p0: String = parts.collect::<Vec<_>>().join(" ");
                    let mut params = Vec::new();
                    if !p0.is_empty() {
                        params.push(p0);
                    }
                    if !ops.is_empty() {
                        ops.remove(0);
                    }
                    params.extend(ops);
                    let mut body = Vec::new();
                    loop {
                        let Some((j, braw)) = it.next() else {
                            return Err(Self::err(file, line_no, ".macro sem .endm"));
                        };
                        let b = strip_comment(braw);
                        if b.trim() == ".endm" {
                            break;
                        }
                        body.push((file.to_string(), j + 1, b.to_string()));
                    }
                    self.macros.insert(name, Macro { params, body });
                }
                m if self.macros.contains_key(m) => {
                    let mac = &self.macros[m];
                    if line.operands.len() != mac.params.len() {
                        return Err(Self::err(
                            file,
                            line_no,
                            format!("macro {m}: esperava {} argumento(s)", mac.params.len()),
                        ));
                    }
                    // rótulos na linha de invocação ficam antes da expansão
                    if !line.labels.is_empty() {
                        self.lines.push(Line {
                            mnemonic: String::new(),
                            operands: vec![],
                            ..line.clone()
                        });
                    }
                    let mut expanded = String::new();
                    for (_, _, b) in &mac.body {
                        let mut s = b.clone();
                        // substitui parâmetros mais longos primeiro (\ab antes de \a)
                        let mut order: Vec<usize> = (0..mac.params.len()).collect();
                        order.sort_by_key(|&k| std::cmp::Reverse(mac.params[k].len()));
                        for k in order {
                            s = s.replace(&format!("\\{}", mac.params[k]), &line.operands[k]);
                        }
                        expanded.push_str(&s);
                        expanded.push('\n');
                    }
                    let tag = format!("{file}:{line_no}<{m}>");
                    self.feed(&tag, &expanded)?;
                }
                _ => self.lines.push(line),
            }
        }
        self.depth -= 1;
        Ok(())
    }
}

// ───────────────────────── expressões ─────────────────────────

struct Eval<'a> {
    cur: &'a HashMap<String, i64>,
    prev: &'a HashMap<String, i64>,
    here: i64,
    stmt: usize,
    /// definições de rótulos locais desta passagem (num, stmt, endereço)
    locals_cur: &'a [(u32, usize, i64)],
    /// idem, da passagem anterior (para referências `Nf`)
    locals_prev: &'a [(u32, usize, i64)],
    unresolved: Vec<String>,
}

struct Parser<'a, 'b> {
    s: &'a [u8],
    i: usize,
    ev: &'a mut Eval<'b>,
}

impl<'a, 'b> Parser<'a, 'b> {
    fn ws(&mut self) {
        while self.i < self.s.len() && (self.s[self.i] as char).is_whitespace() {
            self.i += 1;
        }
    }

    fn peek(&mut self) -> Option<u8> {
        self.ws();
        self.s.get(self.i).copied()
    }

    /// Nível mais baixo: comparações (resultado 1/0), para `.assert`.
    fn expr(&mut self) -> Result<i64, String> {
        let a = self.sum()?;
        self.ws();
        let rest = &self.s[self.i..];
        let (len, f): (usize, fn(i64, i64) -> bool) = if rest.starts_with(b"==") {
            (2, |x, y| x == y)
        } else if rest.starts_with(b"!=") {
            (2, |x, y| x != y)
        } else if rest.starts_with(b"<=") {
            (2, |x, y| x <= y)
        } else if rest.starts_with(b">=") {
            (2, |x, y| x >= y)
        } else if rest.starts_with(b"<") {
            (1, |x, y| x < y)
        } else if rest.starts_with(b">") {
            (1, |x, y| x > y)
        } else {
            return Ok(a);
        };
        self.i += len;
        let b = self.sum()?;
        Ok(f(a, b) as i64)
    }

    fn sum(&mut self) -> Result<i64, String> {
        let mut v = self.term()?;
        loop {
            match self.peek() {
                Some(b'+') => {
                    self.i += 1;
                    v = v.checked_add(self.term()?).ok_or("estouro")?;
                }
                Some(b'-') => {
                    self.i += 1;
                    v = v.checked_sub(self.term()?).ok_or("estouro")?;
                }
                _ => return Ok(v),
            }
        }
    }

    fn term(&mut self) -> Result<i64, String> {
        let mut v = self.unary()?;
        loop {
            match self.peek() {
                Some(b'*') => {
                    self.i += 1;
                    v = v.checked_mul(self.unary()?).ok_or("estouro")?;
                }
                Some(c @ (b'/' | b'%')) => {
                    self.i += 1;
                    let d = self.unary()?;
                    let (a, b) = (Word::new(v), Word::new(d));
                    let (Some(a), Some(b)) = (a, b) else {
                        return Err("divisão fora da janela de palavra".into());
                    };
                    if d == 0 {
                        return Err("divisão por zero".into());
                    }
                    let (q, r) = a.divrem(b);
                    v = if c == b'/' { q.val() } else { r.val() };
                }
                _ => return Ok(v),
            }
        }
    }

    fn unary(&mut self) -> Result<i64, String> {
        match self.peek() {
            Some(b'-') => {
                self.i += 1;
                Ok(-self.unary()?)
            }
            Some(b'+') => {
                self.i += 1;
                self.unary()
            }
            _ => self.primary(),
        }
    }

    fn primary(&mut self) -> Result<i64, String> {
        let Some(c) = self.peek() else {
            return Err("expressão incompleta".into());
        };
        if c == b'(' {
            self.i += 1;
            let v = self.expr()?;
            if self.peek() != Some(b')') {
                return Err("falta ')'".into());
            }
            self.i += 1;
            return Ok(v);
        }
        if c == b'\'' {
            return self.char_lit();
        }
        if c.is_ascii_digit() {
            let start = self.i;
            // literal ternário balanceado: 0t seguido de + 0 -
            if c == b'0' && self.s.get(self.i + 1) == Some(&b't') {
                self.i += 2;
                let mut v: i64 = 0;
                let mut n = 0;
                while let Some(&d) = self.s.get(self.i) {
                    let t = match d {
                        b'+' => 1,
                        b'0' => 0,
                        b'-' => -1,
                        _ => break,
                    };
                    v = v * 3 + t;
                    n += 1;
                    self.i += 1;
                    if n > 27 {
                        return Err("literal ternário com mais de 27 trits".into());
                    }
                }
                if n == 0 {
                    return Err("literal ternário vazio".into());
                }
                return Ok(v);
            }
            while self.i < self.s.len() && (self.s[self.i].is_ascii_digit() || self.s[self.i] == b'_') {
                self.i += 1;
            }
            let digits: String = std::str::from_utf8(&self.s[start..self.i])
                .unwrap()
                .chars()
                .filter(|&ch| ch != '_')
                .collect();
            // referência a rótulo local: 1b / 1f
            if let Some(&sfx) = self.s.get(self.i) {
                let after_ok = self
                    .s
                    .get(self.i + 1)
                    .map_or(true, |&n| !is_ident_char(n as char));
                if (sfx == b'b' || sfx == b'f') && after_ok {
                    self.i += 1;
                    let num: u32 = digits.parse().map_err(|_| "rótulo local inválido")?;
                    return self.local_ref(num, sfx == b'b');
                }
            }
            return digits.parse::<i64>().map_err(|_| format!("número inválido: {digits}"));
        }
        if is_ident_start(c as char) {
            let start = self.i;
            while self.i < self.s.len() && is_ident_char(self.s[self.i] as char) {
                self.i += 1;
            }
            let name = std::str::from_utf8(&self.s[start..self.i]).unwrap();
            if name == "." {
                return Ok(self.ev.here);
            }
            if let Some(&v) = self.ev.cur.get(name).or_else(|| self.ev.prev.get(name)) {
                return Ok(v);
            }
            self.ev.unresolved.push(name.to_string());
            return Ok(0);
        }
        Err(format!("caractere inesperado '{}'", c as char))
    }

    fn local_ref(&mut self, num: u32, back: bool) -> Result<i64, String> {
        let stmt = self.ev.stmt;
        if back {
            if let Some(&(_, _, a)) = self
                .ev
                .locals_cur
                .iter()
                .rev()
                .find(|&&(n, s, _)| n == num && s <= stmt)
            {
                return Ok(a);
            }
            Err(format!("rótulo local {num}b não definido antes"))
        } else {
            if let Some(&(_, _, a)) = self
                .ev
                .locals_prev
                .iter()
                .find(|&&(n, s, _)| n == num && s > stmt)
            {
                return Ok(a);
            }
            self.ev.unresolved.push(format!("{num}f"));
            Ok(0)
        }
    }

    fn char_lit(&mut self) -> Result<i64, String> {
        // self.s[self.i] == '\''
        let rest = std::str::from_utf8(&self.s[self.i + 1..]).map_err(|_| "utf-8")?;
        let mut chars = rest.char_indices();
        let (_, c) = chars.next().ok_or("literal de caractere incompleto")?;
        let (v, used) = if c == '\\' {
            let (_, e) = chars.next().ok_or("escape incompleto")?;
            (unescape(e)? as i64, 1 + e.len_utf8())
        } else {
            (c as i64, c.len_utf8())
        };
        if rest[used..].as_bytes().first() != Some(&b'\'') {
            return Err("falta ' no literal de caractere".into());
        }
        self.i += 1 + used + 1;
        Ok(v)
    }
}

fn unescape(e: char) -> Result<char, String> {
    Ok(match e {
        'n' => '\n',
        't' => '\t',
        'r' => '\r',
        '0' => '\0',
        '\\' => '\\',
        '\'' => '\'',
        '"' => '"',
        _ => return Err(format!("escape desconhecido \\{e}")),
    })
}

fn eval(ev: &mut Eval, s: &str) -> Result<i64, String> {
    let mut p = Parser { s: s.as_bytes(), i: 0, ev };
    let v = p.expr()?;
    if p.peek().is_some() {
        return Err(format!("sobra na expressão: '{}'", &s[p.i..]));
    }
    Ok(v)
}

/// Conteúdo de um literal de string `"..."` como códigos de caractere.
fn string_lit(s: &str) -> Result<Vec<i64>, String> {
    let inner = s
        .strip_prefix('"')
        .and_then(|x| x.strip_suffix('"'))
        .ok_or("esperava \"texto\"")?;
    let mut out = Vec::new();
    let mut it = inner.chars();
    while let Some(c) = it.next() {
        if c == '\\' {
            out.push(unescape(it.next().ok_or("escape incompleto")?)? as i64);
        } else {
            out.push(c as i64);
        }
    }
    Ok(out)
}

// ───────────────────────── montagem ─────────────────────────

#[derive(Default)]
struct Section {
    base: i64,
    words: Vec<Word>,
    src: Vec<String>,
    explicit_base: bool,
}

impl Section {
    fn here(&self) -> i64 {
        self.base + self.words.len() as i64
    }
}

struct Pass {
    syms: HashMap<String, i64>,
    locals: Vec<(u32, usize, i64)>,
    sections: [Section; 2],
    entry: Option<i64>,
    soft: Vec<AsmError>,
    unresolved: Vec<(String, usize, String)>,
}

/// `expr(reg)` → (expr, reg). Offset vazio = 0.
fn split_mem(op: &str) -> Option<(String, i8)> {
    let op = op.trim();
    let close = op.strip_suffix(')')?;
    let open = close.rfind('(')?;
    let reg = isa::reg_from_name(close[open + 1..].trim())?;
    let off = close[..open].trim();
    Some((if off.is_empty() { "0".to_string() } else { off.to_string() }, reg))
}

fn run_pass(lines: &[Line], prev: &HashMap<String, i64>, prev_locals: &[(u32, usize, i64)]) -> Result<Pass, AsmError> {
    let mut p = Pass {
        syms: HashMap::new(),
        locals: Vec::new(),
        sections: [Section::default(), Section::default()],
        entry: None,
        soft: Vec::new(),
        unresolved: Vec::new(),
    };
    // base provisória da seção de dados: a calculada na passagem anterior
    p.sections[1].base = prev.get("__data_start").copied().unwrap_or(0);
    let mut sec = 0usize;

    for (stmt, line) in lines.iter().enumerate() {
        let hard = |msg: String| AsmError { file: line.file.clone(), line: line.line, msg };
        let here = p.sections[sec].here();
        for l in &line.labels {
            if let Ok(num) = l.parse::<u32>() {
                p.locals.push((num, stmt, here));
            } else if p.syms.insert(l.clone(), here).is_some() {
                return Err(hard(format!("símbolo redefinido: {l}")));
            }
        }
        if line.mnemonic.is_empty() {
            continue;
        }
        let m = line.mnemonic.as_str();
        let ops = &line.operands;

        // avalia uma expressão; registra não-resolvidos; erro de sintaxe é fatal
        macro_rules! ev {
            ($s:expr) => {{
                let mut e = Eval {
                    cur: &p.syms,
                    prev,
                    here,
                    stmt,
                    locals_cur: &p.locals,
                    locals_prev: prev_locals,
                    unresolved: Vec::new(),
                };
                let r = eval(&mut e, $s).map_err(|msg| hard(format!("{msg} em '{}'", $s)))?;
                for u in e.unresolved {
                    p.unresolved.push((line.file.clone(), line.line, u));
                }
                r
            }};
        }
        macro_rules! need {
            ($n:expr) => {
                if ops.len() != $n {
                    return Err(hard(format!("{m}: esperava {} operando(s), veio {}", $n, ops.len())));
                }
            };
        }
        macro_rules! reg {
            ($s:expr) => {
                isa::reg_from_name($s.trim())
                    .ok_or_else(|| hard(format!("registrador inválido: '{}'", $s)))?
            };
        }
        macro_rules! emit_word {
            ($w:expr) => {{
                p.sections[sec].words.push($w);
                p.sections[sec].src.push(line.text.clone());
            }};
        }
        // emite instrução; erro de alcance é "suave" (pode sumir no ponto fixo)
        macro_rules! emit {
            ($ins:expr) => {{
                let ins: Instr = $ins;
                match encode(&ins) {
                    Ok(w) => emit_word!(w),
                    Err(e) => {
                        let what = match e {
                            IsaError::ImmRange => format!("imediato fora de alcance ({})", ins.imm),
                            other => format!("{:?}", other),
                        };
                        p.soft.push(hard(format!("{m}: {what}")));
                        emit_word!(Word::ZERO);
                    }
                }
            }};
        }
        macro_rules! word_of {
            ($v:expr) => {{
                let v: i64 = $v;
                match Word::new(v) {
                    Some(w) => w,
                    None => {
                        p.soft.push(hard(format!("valor fora da palavra: {v}")));
                        Word::ZERO
                    }
                }
            }};
        }

        if let Some(dir) = m.strip_prefix('.') {
            match dir {
                "text" => sec = 0,
                "data" => sec = 1,
                "org" => {
                    need!(1);
                    let a = ev!(&ops[0]);
                    let s = &mut p.sections[sec];
                    if s.words.is_empty() {
                        s.base = a;
                        s.explicit_base = true;
                        // rótulos desta mesma linha passam a valer o novo endereço
                        for l in &line.labels {
                            if let Ok(num) = l.parse::<u32>() {
                                if let Some(x) = p.locals.iter_mut().rev().find(|x| x.0 == num && x.1 == stmt) {
                                    x.2 = a;
                                }
                            } else {
                                p.syms.insert(l.clone(), a);
                            }
                        }
                    } else if a >= s.here() {
                        let n = (a - s.here()) as usize;
                        if n > 3_000_000 {
                            return Err(hard(format!(".org longe demais ({n} palavras)")));
                        }
                        for _ in 0..n {
                            s.words.push(Word::ZERO);
                            s.src.push(String::new());
                        }
                    } else {
                        p.soft.push(hard(format!(".org {a} antes da posição atual {}", s.here())));
                    }
                }
                "page" => {
                    need!(0);
                    let s = &mut p.sections[sec];
                    let target = page_start_at_or_after(s.here());
                    for _ in s.here()..target {
                        s.words.push(Word::ZERO);
                        s.src.push(String::new());
                    }
                }
                "equ" => {
                    need!(2);
                    if !is_ident(&ops[0]) {
                        return Err(hard(format!("nome inválido em .equ: {}", ops[0])));
                    }
                    let v = ev!(&ops[1]);
                    if p.syms.insert(ops[0].clone(), v).is_some() {
                        return Err(hard(format!("símbolo redefinido: {}", ops[0])));
                    }
                }
                "word" => {
                    if ops.is_empty() {
                        return Err(hard(".word sem valores".into()));
                    }
                    for o in ops {
                        let v = ev!(o);
                        let w = word_of!(v);
                        emit_word!(w);
                    }
                }
                "zero" | "fill" => {
                    let (n, v) = if dir == "zero" {
                        need!(1);
                        (ev!(&ops[0]), 0)
                    } else {
                        need!(2);
                        (ev!(&ops[0]), ev!(&ops[1]))
                    };
                    if !(0..=3_000_000).contains(&n) {
                        return Err(hard(format!(".{dir}: contagem inválida {n}")));
                    }
                    let w = word_of!(v);
                    for _ in 0..n {
                        emit_word!(w);
                    }
                }
                "string" | "stringz" => {
                    need!(1);
                    for c in string_lit(&ops[0]).map_err(&hard)? {
                        emit_word!(Word::lit(c));
                    }
                    if dir == "stringz" {
                        emit_word!(Word::ZERO);
                    }
                }
                "entry" => {
                    need!(1);
                    p.entry = Some(ev!(&ops[0]));
                }
                "assert" => {
                    need!(1);
                    if ev!(&ops[0]) == 0 {
                        p.soft.push(hard(format!(".assert falhou: {}", ops[0])));
                    }
                }
                _ => return Err(hard(format!("diretiva desconhecida: {m}"))),
            }
            continue;
        }

        // ── pseudo-instruções ──
        match m {
            "nop" => {
                need!(0);
                emit!(Instr::i(Op::Addi, 0, 0, 0));
                continue;
            }
            "li" => {
                need!(2);
                let (a, v) = (reg!(&ops[0]), ev!(&ops[1]));
                emit!(Instr::i(Op::Addi, a, 0, v));
                continue;
            }
            "la" => {
                need!(2);
                let (a, v) = (reg!(&ops[0]), ev!(&ops[1]));
                let w = word_of!(v);
                // divisão pura de dígitos: alto (11 trits) e baixo (16 trits)
                emit!(Instr::i(Op::Lui, a, 0, w.field(isa::LUI_SHIFT, 11)));
                emit!(Instr::i(Op::Addi, a, a, w.field(0, 16)));
                continue;
            }
            "mov" => {
                need!(2);
                emit!(Instr::i(Op::Addi, reg!(&ops[0]), reg!(&ops[1]), 0));
                continue;
            }
            "neg" => {
                need!(2);
                emit!(Instr::r(Op::Sub, reg!(&ops[0]), 0, reg!(&ops[1])));
                continue;
            }
            "sgn" => {
                need!(2);
                emit!(Instr::r(Op::Cmp, reg!(&ops[0]), reg!(&ops[1]), 0));
                continue;
            }
            "j" => {
                need!(1);
                let t = ev!(&ops[0]);
                emit!(Instr::j(Op::Jal, 0, t - here));
                continue;
            }
            "call" => {
                need!(1);
                let t = ev!(&ops[0]);
                emit!(Instr::j(Op::Jal, isa::LR, t - here));
                continue;
            }
            "ret" => {
                need!(0);
                emit!(Instr::i(Op::Jalr, 0, isa::LR, 0));
                continue;
            }
            "jr" => {
                need!(1);
                emit!(Instr::i(Op::Jalr, 0, reg!(&ops[0]), 0));
                continue;
            }
            "bz" | "bnz" | "bn" | "bp" | "bnn" | "bnp" => {
                need!(2);
                let op = match m {
                    "bz" => Op::Beq,
                    "bnz" => Op::Bne,
                    "bn" => Op::Blt,
                    "bp" => Op::Bgt,
                    "bnn" => Op::Bge,
                    _ => Op::Ble,
                };
                let (a, t) = (reg!(&ops[0]), ev!(&ops[1]));
                emit!(Instr::i(op, a, 0, t - here));
                continue;
            }
            "push" => {
                need!(1);
                let a = reg!(&ops[0]);
                emit!(Instr::i(Op::Addi, isa::SP, isa::SP, -1));
                emit!(Instr::i(Op::St, a, isa::SP, 0));
                continue;
            }
            "pop" => {
                need!(1);
                let a = reg!(&ops[0]);
                emit!(Instr::i(Op::Ld, a, isa::SP, 0));
                emit!(Instr::i(Op::Addi, isa::SP, isa::SP, 1));
                continue;
            }
            _ => {}
        }

        // ── instruções reais ──
        let Some(op) = Op::from_mnemonic(m) else {
            return Err(hard(format!("mnemônico desconhecido: {m}")));
        };
        let csr_of = |s: &str, p: &mut Pass| -> Result<i64, AsmError> {
            if let Some(n) = csr::from_name(s.trim()) {
                return Ok(n);
            }
            let mut e = Eval {
                cur: &p.syms,
                prev,
                here,
                stmt,
                locals_cur: &p.locals,
                locals_prev: prev_locals,
                unresolved: Vec::new(),
            };
            let r = eval(&mut e, s).map_err(|msg| hard(format!("{msg} em '{s}'")))?;
            for u in e.unresolved {
                p.unresolved.push((line.file.clone(), line.line, u));
            }
            Ok(r)
        };
        match op {
            Op::Ld | Op::St | Op::Jalr => {
                need!(2);
                let a = reg!(&ops[0]);
                let (off, b) = split_mem(&ops[1])
                    .ok_or_else(|| hard(format!("{m}: esperava desloc(reg), veio '{}'", ops[1])))?;
                let imm = ev!(&off);
                emit!(Instr::i(op, a, b, imm));
            }
            Op::Beq | Op::Bne | Op::Blt | Op::Bge | Op::Bgt | Op::Ble => {
                need!(3);
                let (a, b, t) = (reg!(&ops[0]), reg!(&ops[1]), ev!(&ops[2]));
                emit!(Instr::i(op, a, b, t - here));
            }
            Op::Jal => {
                if ops.len() == 1 {
                    let t = ev!(&ops[0]);
                    emit!(Instr::j(op, isa::LR, t - here));
                } else {
                    need!(2);
                    let (a, t) = (reg!(&ops[0]), ev!(&ops[1]));
                    emit!(Instr::j(op, a, t - here));
                }
            }
            Op::Lui => {
                need!(2);
                let (a, v) = (reg!(&ops[0]), ev!(&ops[1]));
                emit!(Instr::i(op, a, 0, v));
            }
            Op::Addi | Op::Shi => {
                need!(3);
                let (a, b, v) = (reg!(&ops[0]), reg!(&ops[1]), ev!(&ops[2]));
                emit!(Instr::i(op, a, b, v));
            }
            Op::Csrr => {
                need!(2);
                let a = reg!(&ops[0]);
                let n = csr_of(&ops[1], &mut p)?;
                emit!(Instr::i(op, a, 0, n));
            }
            Op::Csrw => {
                need!(2);
                let n = csr_of(&ops[0], &mut p)?;
                let b = reg!(&ops[1]);
                emit!(Instr::i(op, 0, b, n));
            }
            Op::Csrx => {
                need!(3);
                let a = reg!(&ops[0]);
                let n = csr_of(&ops[1], &mut p)?;
                let b = reg!(&ops[2]);
                emit!(Instr::i(op, a, b, n));
            }
            _ => match op.fmt() {
                Fmt::R => {
                    need!(3);
                    emit!(Instr::r(op, reg!(&ops[0]), reg!(&ops[1]), reg!(&ops[2])));
                }
                Fmt::N => {
                    need!(0);
                    emit!(Instr::n(op));
                }
                _ => unreachable!("formato sem tratador: {m}"),
            },
        }
    }

    // símbolos de layout
    let text_end = p.sections[0].here();
    let data_start = if p.sections[1].explicit_base {
        p.sections[1].base
    } else {
        page_start_at_or_after(text_end)
    };
    p.syms.insert("__text_start".into(), p.sections[0].base);
    p.syms.insert("__text_end".into(), text_end);
    p.syms.insert("__data_start".into(), data_start);
    p.syms.insert("__data_end".into(), data_start + p.sections[1].words.len() as i64);
    Ok(p)
}

fn finish(lines: Vec<Line>) -> Result<Program, AsmError> {
    let mut prev: HashMap<String, i64> = HashMap::new();
    let mut prev_locals: Vec<(u32, usize, i64)> = Vec::new();
    for _ in 0..24 {
        let p = run_pass(&lines, &prev, &prev_locals)?;
        let stable = p.syms == prev && p.locals == prev_locals;
        if stable {
            if let Some((file, line, name)) = p.unresolved.into_iter().next() {
                return Err(AsmError { file, line, msg: format!("símbolo indefinido: {name}") });
            }
            if let Some(e) = p.soft.into_iter().next() {
                return Err(e);
            }
            let [text, data] = p.sections;
            for (name, s) in [("texto", &text), ("dados", &data)] {
                if s.base.abs() > M27 || s.here().abs() > M27 {
                    return Err(AsmError { file: name.into(), line: 0, msg: "seção fora do espaço de endereços".into() });
                }
            }
            if !data.words.is_empty() {
                let (t0, t1, d0, d1) = (text.base, text.here(), data.base, data.here());
                if t0 < d1 && d0 < t1 {
                    return Err(AsmError { file: "layout".into(), line: 0, msg: format!("texto [{t0},{t1}) e dados [{d0},{d1}) se sobrepõem") });
                }
            }
            let mut listing = Vec::new();
            for s in [&text, &data] {
                for (i, (&w, src)) in s.words.iter().zip(&s.src).enumerate() {
                    listing.push((s.base + i as i64, w, src.clone()));
                }
            }
            let symbols: BTreeMap<String, i64> = p.syms.into_iter().collect();
            let entry = p
                .entry
                .or_else(|| symbols.get("_start").copied())
                .unwrap_or(text.base);
            let mut segments = vec![Segment { base: text.base, words: text.words, perm: Perm::Rx }];
            if !data.words.is_empty() {
                segments.push(Segment { base: data.base, words: data.words, perm: Perm::Rw });
            }
            return Ok(Program { segments, symbols, entry, listing });
        }
        prev = p.syms;
        prev_locals = p.locals;
    }
    Err(AsmError { file: "layout".into(), line: 0, msg: "montagem não convergiu (símbolos circulares?)".into() })
}

/// Monta um texto-fonte. `base_dir` resolve `.include`.
pub fn assemble_str(src: &str, name: &str, base_dir: Option<&Path>) -> Result<Program, AsmError> {
    let mut pre = Pre {
        lines: Vec::new(),
        macros: HashMap::new(),
        base_dir: base_dir.map(Path::to_path_buf),
        depth: 0,
    };
    pre.feed(name, src)?;
    finish(pre.lines)
}

/// Monta um arquivo (o diretório dele resolve `.include`).
pub fn assemble_file(path: &Path) -> Result<Program, AsmError> {
    let src = std::fs::read_to_string(path).map_err(|e| AsmError {
        file: path.display().to_string(),
        line: 0,
        msg: format!("não li o arquivo: {e}"),
    })?;
    let name = path.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    assemble_str(&src, &name, path.parent())
}

/// 3¹⁶ — peso do campo alto de `la` (exposto para testes).
pub const LA_HI_WEIGHT: i64 = POW3[isa::LUI_SHIFT];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::isa::{decode, disasm};

    fn asm(src: &str) -> Program {
        assemble_str(src, "t", None).unwrap_or_else(|e| panic!("{e}"))
    }

    #[test]
    fn basic_program_and_labels() {
        let p = asm("
            .org 100
        _start: li a1, 5
        loop:   addi a1, a1, -1
                bnz a1, loop
                j done
                nop
        done:   sys
        ");
        assert_eq!(p.entry, 100);
        assert_eq!(p.sym("loop"), 101);
        assert_eq!(p.sym("done"), 105);
        let t = p.text();
        assert_eq!(t.base, 100);
        assert_eq!(decode(t.words[2]).unwrap(), Instr::i(Op::Bne, 1, 0, -1));
        assert_eq!(decode(t.words[3]).unwrap(), Instr::j(Op::Jal, 0, 2));
    }

    #[test]
    fn la_splits_digits_exactly() {
        // qualquer constante = hi·3¹⁶ + lo, SEM ajuste de empréstimo
        for v in [0i64, 1, -1, 21_523_360, 21_523_361, -21_523_361, M27, -M27, 14_348_907, 123_456_789_012] {
            let p = asm(&format!("la a3, {v}"));
            let hi = decode(p.text().words[0]).unwrap();
            let lo = decode(p.text().words[1]).unwrap();
            assert_eq!(hi.op, Op::Lui);
            assert_eq!(lo.op, Op::Addi);
            assert_eq!(hi.imm * LA_HI_WEIGHT + lo.imm, v, "la {v}");
        }
    }

    #[test]
    fn data_section_starts_on_page_boundary() {
        let p = asm("
            .org 365
            la a1, msg
            ld a2, 0(a1)
            .data
        msg: .string \"oi\"
        cnt: .word 7, -7
        ");
        assert_eq!(p.text().base, 365);
        let d = p.data().unwrap();
        assert_eq!(d.base, 729 + 365); // início da página 2
        assert_eq!(p.sym("msg"), d.base);
        assert_eq!(p.sym("cnt"), d.base + 2);
        assert_eq!(d.words[0].val(), 'o' as i64);
        assert_eq!(d.words[3].val(), -7);
        // la resolveu o endereço de dados (referência para frente entre seções)
        let hi = decode(p.text().words[0]).unwrap();
        let lo = decode(p.text().words[1]).unwrap();
        assert_eq!(hi.imm * LA_HI_WEIGHT + lo.imm, d.base);
    }

    #[test]
    fn local_labels_macros_and_equ() {
        let p = asm("
            .equ N, 3
            K = N * 2 + 1
            .macro DEC r
                addi \\r, \\r, -1
            .endm
                li a1, K
            1:  DEC a1
                bnz a1, 1b
                bz a1, 1f
                nop
            1:  ret
        ");
        let w = &p.text().words;
        assert_eq!(decode(w[0]).unwrap().imm, 7);
        assert_eq!(decode(w[2]).unwrap(), Instr::i(Op::Bne, 1, 0, -1));
        assert_eq!(decode(w[3]).unwrap(), Instr::i(Op::Beq, 1, 0, 2));
        assert_eq!(disasm(&decode(w[5]).unwrap(), None), "jalr zr, 0(lr)");
    }

    #[test]
    fn expressions() {
        let p = asm("
            .word 0t+0-, 'A', '\\n', (2+3)*4, -(5), 5/3, 5%3, 7/2
            .word 3 == 3, 3 != 3, 2 < 3, (1 + 1) >= 3
            .assert . - __text_start == 12
        ");
        let v: Vec<i64> = p.text().words.iter().map(|w| w.val()).collect();
        // 5/3 = 2 e 5%3 = −1: divisão centrada, como a instrução DIV
        assert_eq!(v, vec![8, 65, 10, 20, -5, 2, -1, 3, 1, 0, 1, 0]);
        assert!(assemble_str(".assert 1 == 2", "t", None).unwrap_err().msg.contains(".assert"));
    }

    #[test]
    fn errors_are_reported() {
        let e = assemble_str("  li a1, 99999999999", "t", None).unwrap_err();
        assert!(e.msg.contains("alcance"), "{e}");
        let e = assemble_str("  j nowhere", "t", None).unwrap_err();
        assert!(e.msg.contains("indefinido"), "{e}");
        let e = assemble_str("  frob a1", "t", None).unwrap_err();
        assert!(e.msg.contains("desconhecido"), "{e}");
        let e = assemble_str("x: nop\nx: nop", "t", None).unwrap_err();
        assert!(e.msg.contains("redefinido"), "{e}");
    }

    #[test]
    fn disassembly_reassembles_to_same_word() {
        // montar ∘ desmontar = identidade nas instruções válidas (amostra de formas)
        let src = "
            add a1, a2, a3
            addi sp, sp, -40
            lui t1, -88573
            ld a1, -13(t4)
            st lr, 364(sp)
            csrx t4, scratch, t4
            csrr a1, cause
            csrw tvec, a2
            shi a1, a1, -6
            kmin s1, s2, s3
            eret
        ";
        let p = asm(src);
        for &w in &p.text().words {
            let text = disasm(&decode(w).unwrap(), None);
            let q = asm(&text);
            assert_eq!(q.text().words[0], w, "{text}");
        }
    }
}
