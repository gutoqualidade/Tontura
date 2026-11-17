//! TENERURA — léxico do subconjunto de Rust para T27 (`.t27r`).
//!
//! Só o que o subconjunto usa: identificadores, literais decimais,
//! palavras-chave, pontuação e os operadores. Comentários `//` até o fim
//! da linha. Nada de strings, floats, macros nem atributos.

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Tok {
    Ident(String),
    Num(i64),
    // palavras-chave
    Fn,
    Let,
    Mut,
    If,
    Else,
    While,
    Return,
    Match,
    True,
    False,
    Word,
    // pontuação
    Arrow,
    FatArrow,
    LParen,
    RParen,
    LBrace,
    RBrace,
    Comma,
    Semi,
    Colon,
    // operadores
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    EqEq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    AndAnd,
    OrOr,
    Pipe,
    Bang,
    Assign,
    Underscore,
    Eof,
}

#[derive(Clone, Debug)]
pub struct Token {
    pub tok: Tok,
    pub line: usize,
    pub col: usize,
}

pub struct LexError {
    pub line: usize,
    pub col: usize,
    pub msg: String,
}

pub fn lex(src: &str) -> Result<Vec<Token>, LexError> {
    let chars: Vec<char> = src.chars().collect();
    let (mut i, mut line, mut col) = (0usize, 1usize, 1usize);
    let mut out = Vec::new();
    let err = |line: usize, col: usize, msg: String| LexError { line, col, msg };

    // avança um caractere mantendo linha/coluna
    macro_rules! bump {
        () => {{
            if chars[i] == '\n' {
                line += 1;
                col = 1;
            } else {
                col += 1;
            }
            i += 1;
        }};
    }

    while i < chars.len() {
        let c = chars[i];
        // espaços
        if c == ' ' || c == '\t' || c == '\r' || c == '\n' {
            bump!();
            continue;
        }
        // comentário de linha
        if c == '/' && chars.get(i + 1) == Some(&'/') {
            while i < chars.len() && chars[i] != '\n' {
                bump!();
            }
            continue;
        }
        let (l0, c0) = (line, col);
        // identificador / palavra-chave
        if c.is_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                bump!();
            }
            let s: String = chars[start..i].iter().collect();
            let tok = match s.as_str() {
                "fn" => Tok::Fn,
                "let" => Tok::Let,
                "mut" => Tok::Mut,
                "if" => Tok::If,
                "else" => Tok::Else,
                "while" => Tok::While,
                "return" => Tok::Return,
                "match" => Tok::Match,
                "true" => Tok::True,
                "false" => Tok::False,
                "word" => Tok::Word,
                "_" => Tok::Underscore,
                _ => Tok::Ident(s),
            };
            out.push(Token { tok, line: l0, col: c0 });
            continue;
        }
        // número decimal
        if c.is_ascii_digit() {
            let start = i;
            while i < chars.len() && chars[i].is_ascii_digit() {
                bump!();
            }
            let s: String = chars[start..i].iter().collect();
            let v: i64 = s
                .parse()
                .map_err(|_| err(l0, c0, format!("literal grande demais: {s}")))?;
            out.push(Token { tok: Tok::Num(v), line: l0, col: c0 });
            continue;
        }
        // operadores e pontuação (dois caracteres primeiro)
        let two = |a: char, b: char| c == a && chars.get(i + 1) == Some(&b);
        let (tok, len) = if two('-', '>') {
            (Tok::Arrow, 2)
        } else if two('=', '>') {
            (Tok::FatArrow, 2)
        } else if two('=', '=') {
            (Tok::EqEq, 2)
        } else if two('!', '=') {
            (Tok::Ne, 2)
        } else if two('<', '=') {
            (Tok::Le, 2)
        } else if two('>', '=') {
            (Tok::Ge, 2)
        } else if two('&', '&') {
            (Tok::AndAnd, 2)
        } else if two('|', '|') {
            (Tok::OrOr, 2)
        } else {
            let t = match c {
                '(' => Tok::LParen,
                ')' => Tok::RParen,
                '{' => Tok::LBrace,
                '}' => Tok::RBrace,
                ',' => Tok::Comma,
                ';' => Tok::Semi,
                ':' => Tok::Colon,
                '+' => Tok::Plus,
                '-' => Tok::Minus,
                '*' => Tok::Star,
                '/' => Tok::Slash,
                '%' => Tok::Percent,
                '<' => Tok::Lt,
                '>' => Tok::Gt,
                '!' => Tok::Bang,
                '=' => Tok::Assign,
                '|' => Tok::Pipe,
                _ => return Err(err(l0, c0, format!("caractere inesperado: {c:?}"))),
            };
            (t, 1)
        };
        for _ in 0..len {
            bump!();
        }
        out.push(Token { tok, line: l0, col: c0 });
    }
    out.push(Token { tok: Tok::Eof, line, col });
    Ok(out)
}
