//! TENERURA — testes da linguagem de sistemas (subconjunto de Rust → T27).
//!
//! Cada teste compila fonte `.t27r` com `lang::compile`, monta o resultado
//! com o montador real e executa sob o núcleo real, com os invariantes
//! conferidos a cada saída do núcleo. O que se prova aqui é o caminho
//! inteiro: fonte → assembly → imagem → máquina → console/código de saída.

mod common;
use common::*;

use tenerura_t27::lang;

type Compiler = fn(&str) -> Result<String, lang::Diag>;

/// Os dois geradores: máquina de pilha direto da AST (oráculo), e via TIR
/// (o caminho de compilação, docs/13).
/// Todo programa executável roda nos dois e o resultado tem de ser igual.
const BACKENDS: [(&str, Compiler); 2] = [("pilha", lang::compile_pilha), ("tir", lang::compile)];

/// Compila e monta; devolve o programa de usuário.
fn build_with(c: Compiler, src: &str) -> Result<tenerura_t27::asm::Program, String> {
    let tas = c(src).map_err(|d| format!("{d}"))?;
    tenerura_t27::asm::assemble_str(&tas, "gerado", Some(&os_dir().join("user")))
        .map_err(|e| format!("montagem: {e}\n--- .tas ---\n{tas}"))
}

/// Roda um programa sozinho, com console, nos dois geradores; confere que
/// concordam e devolve (saída, código).
fn run_with(src: &str, caps: impl Fn(tenerura_t27::sysimage::TaskSpec) -> tenerura_t27::sysimage::TaskSpec) -> (String, i64) {
    let mut seen = Vec::new();
    for (name, c) in BACKENDS {
        let prog = build_with(c, src).unwrap_or_else(|e| panic!("{name}: {e}"));
        let mut s = boot_k(&kernel(), vec![caps(task("p", prog)).con()], 0, None).unwrap();
        let dead = s.k("ST_DEAD");
        run_checked(&mut s, 400_000, 64, |s| s.task("p").state == dead)
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        seen.push((s.m.console_text(), s.task("p").exit, s.m.stats.instr_user));
    }
    let ((out, code, pilha), (out2, code2, tir)) = (seen[0].clone(), seen[1].clone());
    assert_eq!((&out, code), (&out2, code2), "os geradores divergem");
    eprintln!("instruções de usuário: pilha {pilha}, tir {tir}");
    assert!(tir <= pilha, "o TIR gerou código mais lento: {tir} > {pilha}");
    (out, code)
}

fn run(src: &str) -> (String, i64) {
    run_with(src, |t| t)
}

/// Código de saída de um programa que só calcula.
fn exit_of(src: &str) -> i64 {
    run(src).1
}

/// Mensagem de erro de compilação (sem o prefixo de posição).
fn err_of(src: &str) -> String {
    match lang::compile(src) {
        Ok(_) => panic!("esperava erro de compilação, mas compilou"),
        Err(d) => d.msg,
    }
}

#[test]
fn laco_soma_e_imprime() {
    let (out, code) = run(
        "fn main() -> word {
            let mut i = 0;
            let mut s = 0;
            while i < 10 {
                i = i + 1;
                s = s + i;
            }
            putdec(s);
            putc(10);
            return s;
        }",
    );
    assert_eq!(code, 55);
    assert_eq!(out, "55\n");
}

#[test]
fn recursao_e_match_de_tres_vias() {
    let code = exit_of(
        "fn fato(n: word) -> word {
            if n <= 1 { return 1; }
            return n * fato(n - 1);
        }
        fn sinal(x: word) -> word {
            match sign(x) {
                -1 => { return 100; }
                0 => { return 200; }
                _ => { return 300; }
            }
        }
        fn main() -> word {
            return fato(5) + sinal(0 - 7) + sinal(0) + sinal(9);
        }",
    );
    assert_eq!(code, 720, "120 + 100 + 200 + 300");
}

#[test]
fn precedencia_comparacoes_e_logica() {
    // cada parcela vale 1 se a expressão estiver certa
    let code = exit_of(
        "fn main() -> word {
            let mut r = 0;
            if (2 + 3) * 4 == 20 { r = r + 1; }
            if 10 / 3 == 3 { r = r + 1; }
            if 10 % 3 == 1 { r = r + 1; }
            if 2 - 3 - 4 == 0 - 5 { r = r + 1; }
            if (1 < 2) && (2 <= 2) && !(3 > 4) { r = r + 1; }
            // docs/18 §4.2: palavra não é condição; a comparação é explícita
            if (0 != 0 || 7 != 0) && !(0 != 0) { r = r + 1; }
            return r;
        }",
    );
    assert_eq!(code, 6);
}

#[test]
fn literais_maiores_que_16_trits() {
    // 10^9 não cabe em imediato de 16 trits: força o caminho `la`
    let code = exit_of(
        "fn main() -> word {
            let x = 1000000000;
            return x / 1000000;
        }",
    );
    assert_eq!(code, 1000);
}

#[test]
fn parametros_e_chamadas_com_varios_argumentos() {
    let code = exit_of(
        "fn soma5(a: word, b: word, c: word, d: word, e: word) -> word {
            return a + b + c + d + e;
        }
        fn main() -> word {
            return soma5(1, 2, 3, 4, 5);
        }",
    );
    assert_eq!(code, 15);
}

#[test]
fn chamada_de_sistema_pelo_programa() {
    // recv sem remetente e sem paciência devolve 0 ("nada aconteceu")
    let (_, code) = run_with(
        "fn main() -> word {
            let r = recv(0, 0);
            return r + 10;
        }",
        |t| t.recv(0, 1),
    );
    assert_eq!(code, 10);
}

#[test]
fn ping_em_t27r_fala_com_o_servidor_em_assembly() {
    // o exemplo do repositório: cliente escrito na linguagem, servidor em
    // assembly, protocolo (op=1, valor=i) -> i+1, n=10, soma = 65.
    let src = std::fs::read_to_string(os_dir().join("user/pingr.t27r")).expect("pingr.t27r");
    for (name, c) in BACKENDS {
        let prog = build_with(c, &src).unwrap_or_else(|e| panic!("{name}: {e}"));
        let mut s = boot_k(
            &kernel(),
            vec![task("ping", prog).send(0, 1, 7).con(), task("pong", user("pong")).recv(0, 1)],
            0,
            None,
        )
        .unwrap();
        let dead = s.k("ST_DEAD");
        run_checked(&mut s, 400_000, 64, |s| s.task("ping").state == dead && s.task("pong").state == dead)
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(s.m.console_text(), "ping: soma=65\n", "{name}");
        assert_eq!(s.task("ping").exit, 0, "{name}");
        assert_eq!(s.task("pong").exit, 0, "{name}");
        eprintln!("{name}: {} passos, {} instruções de usuário", s.m.stats.instr_user + s.m.stats.instr_kernel, s.m.stats.instr_user);
    }
}

#[test]
fn servidor_em_t27r_recebe_de_cliente_em_assembly() {
    // um cliente em assembly entrega duas palavras (41 e 0) por SEND; o
    // servidor escrito em T27R soma as não nulas e encerra no 0.
    let src = "fn main() -> word {
        let mut soma = 0;
        while true {
            let v = recv_val(0, -1);
            if v == 0 {
                exit(soma);
            }
            soma = soma + v;
        }
        return soma;
    }";
    for (name, c) in BACKENDS {
        let servidor = build_with(c, src).unwrap_or_else(|e| panic!("{name}: {e}"));
        let cliente = prog(
        "    li a1, SYS_SEND
    li a2, 0
    li a3, 41
    li t1, FOREVER
    sys
    bnp a1, fail
    li a1, SYS_SEND
    li a2, 0
    li a3, 0
    li t1, FOREVER
    sys
    bnp a1, fail
    li a2, 0
    j exit
fail:
    li a2, 1
    j exit",
        );
        let mut s = boot_k(
            &kernel(),
            vec![task("srv", servidor).recv(0, 1), task("cli", cliente).send(0, 1, 0)],
            0,
            None,
        )
        .unwrap();
        let dead = s.k("ST_DEAD");
        run_checked(&mut s, 400_000, 64, |s| s.task("srv").state == dead && s.task("cli").state == dead)
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(s.task("srv").exit, 41, "{name}");
        assert_eq!(s.task("cli").exit, 0, "{name}");
    }
}

#[test]
fn atribuicao_a_imutavel_e_recusada() {
    let e = err_of(
        "fn main() -> word {
            let x = 1;
            x = 2;
            return x;
        }",
    );
    assert!(e.contains("não é mutável"), "{e}");
}

#[test]
fn nome_inexistente_e_recusado() {
    let e = err_of("fn main() -> word { return y; }");
    assert!(e.contains("'y' não existe"), "{e}");
}

#[test]
fn aridade_errada_e_recusada() {
    let e = err_of(
        "fn f(a: word) -> word { return a; }
        fn main() -> word { return f(1, 2); }",
    );
    assert!(e.contains("espera 1 argumento"), "{e}");
}

#[test]
fn match_sem_curinga_e_recusado() {
    let e = err_of(
        "fn main() -> word {
            match 1 { 1 => { return 1; } }
            return 0;
        }",
    );
    assert!(e.contains("match sem '_'"), "{e}");
}

#[test]
fn exit_como_valor_e_recusado() {
    let e = err_of("fn main() -> word { let x = exit(0); return x; }");
    assert!(e.contains("exit não retorna"), "{e}");
}

#[test]
fn padrao_repetido_no_match_e_recusado() {
    let e = err_of(
        "fn main() -> word {
            match 1 { 1 => { return 1; } 1 | 2 => { return 2; } _ => { return 0; } }
            return 0;
        }",
    );
    assert!(e.contains("repetido"), "{e}");
}

#[test]
fn falta_main_e_recusada() {
    let e = err_of("fn f() -> word { return 1; }");
    assert!(e.contains("falta 'fn main"), "{e}");
}

#[test]
fn o_programa_gerado_nao_quebra_invariante_do_nucleo() {
    // `run_checked` já confere I1..I12 a cada saída do núcleo; aqui só se
    // garante que um programa com laço, chamadas e pilha não viola nada.
    let (out, code) = run(
        "fn fib(n: word) -> word {
            if n < 2 { return n; }
            return fib(n - 1) + fib(n - 2);
        }
        fn main() -> word {
            let mut i = 0;
            while i < 10 {
                putdec(fib(i));
                putc(32);
                i = i + 1;
            }
            return fib(10);
        }",
    );
    assert_eq!(out, "0 1 1 2 3 5 8 13 21 34 ");
    assert_eq!(code, 55);
}

#[test]
fn tres_vias_e_match_exaustivo_de_trit() {
    // `<=>` é uma instrução (cmp) e devolve trit; `match` sobre trit cobre
    // −1, 0 e 1 sem `_`; bool e trit atravessam parâmetros e retornos
    let (out, code) = run(
        "fn classe(x: word, y: word) -> trit { return x <=> y; }
        fn peso(t: trit) -> word {
            match t {
                -1 => { return 100; }
                0 => { return 20; }
                1 => { return 3; }
            }
            return 0;
        }
        fn par(x: word) -> bool { return x % 2 == 0; }
        fn conta(b: bool, n: word) -> word { if b { return n + 1; } return n; }
        fn main() -> word {
            putdec(peso(classe(1, 2)) + peso(classe(5, 5)) + peso(classe(9, -9)));
            let mut n = 0;
            let mut i = 0;
            while i < 10 { n = conta(par(i), n); i = i + 1; }
            let t: trit = -1;
            return n + peso(t) + peso(sign(-7) <=> 0);
        }",
    );
    assert_eq!(out, "123");
    assert_eq!(code, 5 + 100 + 100);
}

#[test]
fn logica_de_kleene_com_kmin_e_kmax() {
    // tabela inteira: & = mínimo, | = máximo, e De Morgan com a negação
    // exata; em bool, & e | são E/OU sem curto-circuito
    let code = exit_of(
        "fn menor(x: word, y: word) -> word { if x < y { return x; } return y; }
        fn maior(x: word, y: word) -> word { if x > y { return x; } return y; }
        fn main() -> word {
            let mut ok = 0;
            let mut i = -1;
            while i <= 1 {
                let mut j = -1;
                while j <= 1 {
                    let a = sign(i);
                    let b = sign(j);
                    if (a & b) == menor(i, j) { ok = ok + 1; }
                    if (a | b) == maior(i, j) { ok = ok + 1; }
                    if -(a & b) == (-a | -b) { ok = ok + 1; }
                    j = j + 1;
                }
                i = i + 1;
            }
            if (1 < 2) & !(3 < 2) { ok = ok + 100; }
            if (1 > 2) | (2 > 1) { ok = ok + 100; }
            return ok;
        }",
    );
    assert_eq!(code, 27 + 200);
}

#[test]
fn funcao_sem_valor_de_retorno() {
    let (out, code) = run(
        "fn linha(x: word) { putdec(x); putc(10); }
        fn talvez(x: word) { if x < 0 { return; } linha(x); }
        fn main() -> word { linha(7); talvez(-3); talvez(4); return 0; }",
    );
    assert_eq!(out, "7\n4\n");
    assert_eq!(code, 0);
}

#[test]
fn tipos_errados_sao_recusados() {
    for (src, msg) in [
        ("fn main() -> word { let x = 7; if x { return 1; } return 0; }", "a condição tem de ser bool"),
        ("fn main() -> word { while 1 { } return 0; }", "a condição tem de ser bool"),
        ("fn main() -> word { if !0 { return 1; } return 0; }", "'!' sobre word"),
        ("fn main() -> word { let b = 1 < 2; return b + 1; }", "só números"),
        ("fn main() -> word { let t: trit = 5; return t; }", "esperava trit, achei word"),
        ("fn main() -> word { let x = 4; return x & 1; }", "só trit com trit"),
        ("fn main() -> word { if 1 && 2 { return 1; } return 0; }", "só bool"),
        ("fn main() -> word { match sign(3) { -1 => { } 0 => { } } return 0; }", "não exaustivo"),
        ("fn main() -> word { match sign(3) { 2 => { } _ => { } } return 0; }", "impossível"),
        ("fn f() { } fn main() -> word { return f(); }", "nada (função sem '->')"),
        ("fn main() -> word { return; }", "falta o valor de retorno"),
        ("fn f() { return 1; } fn main() -> word { return 0; }", "'return' sem valor"),
        ("fn main() { }", "main devolve o código de saída"),
        ("fn main() -> word { let loop = 1; return loop; }", "palavra reservada"),
        ("fn f(b: bool) -> word { return 0; } fn main() -> word { return f(1); }", "esperava bool, achei word"),
    ] {
        let e = err_of(src);
        assert!(e.contains(msg), "{src}\n→ {e}");
    }
}
