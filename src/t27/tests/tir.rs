//! TENERURA — testes do TIR (docs/13): formato binário, validação,
//! determinismo, paralelismo e operações de ponteiro.
//!
//! A equivalência semântica T27R → TIR → .tas está em `tests/lang.rs`:
//! todo programa executável de lá roda nos dois geradores.

mod common;
use common::*;

use tenerura_t27::lang::{self, ir, irgen};
use ir::{Func, Inst, Module, Op, Region};

/// Programa sintético com todas as construções: `n` funções encadeadas.
fn synth(n: usize) -> String {
    let mut s = String::new();
    for k in 0..n {
        let next = if k + 1 < n { format!("f{}(n - 1)", k + 1) } else { "0".into() };
        s += &format!(
            "fn f{k}(n: word) -> word {{
                let mut i = 0;
                let mut s = {k};
                while i < n && s < 1000000 {{
                    match sign(i - 5) {{
                        -1 => {{ s = s + i; }}
                        0 => {{ s = s * 2; }}
                        _ => {{ if (i % 3 == 0) || !(s > 10) {{ s = s - 1; }} else {{ s = s + 2; }} }}
                    }}
                    i = i + 1;
                }}
                if n <= 0 {{ return s; }}
                return s + {next};
            }}\n"
        );
    }
    s + "fn main() -> word { return f0(3); }\n"
}

#[test]
fn mesma_entrada_mesmos_bytes_e_ida_e_volta_exata() {
    let src = synth(20);
    let (a, b) = (lang::to_ir(&src).unwrap_or_else(|d| panic!("{d}")), lang::to_ir(&src).unwrap_or_else(|d| panic!("{d}")));
    let bytes = a.encode();
    assert_eq!(bytes, b.encode(), "duas compilações, bytes diferentes");
    let back = Module::decode(&bytes).unwrap();
    assert_eq!(back, a, "decode ∘ encode ≠ identidade");
    assert_eq!(back.encode(), bytes, "encode ∘ decode ≠ identidade");
    ir::validate(&back, 1).unwrap();
    assert_eq!(ir::print(&back), ir::print(&a));
}

#[test]
fn paralelo_e_sequencial_dao_o_mesmo_resultado() {
    let m = lang::to_ir(&synth(300)).unwrap_or_else(|d| panic!("{d}"));
    ir::validate(&m, 8).unwrap();
    assert_eq!(irgen::generate(&m, 1).unwrap(), irgen::generate(&m, 8).unwrap());
    // o primeiro erro é o mesmo com 1 ou 8 threads
    let mut bad = m.clone();
    let last = bad.insts.len() - 1;
    bad.insts[last].a = u32::MAX; // return de um valor que não existe
    let mid = bad.funcs[150].ifirst as usize + bad.funcs[150].icount as usize - 1;
    bad.insts[mid].a = u32::MAX;
    let (e1, e8) = (ir::validate(&bad, 1).unwrap_err(), ir::validate(&bad, 8).unwrap_err());
    assert_eq!(e1, e8);
    assert!(e1.starts_with("fn #150:"), "{e1}");
}

#[test]
fn bytes_corrompidos_nunca_derrubam_decode_validacao_nem_gerador() {
    // entrada não confiável: toda mutação de 1 byte e todo truncamento tem
    // de terminar em Ok ou Err — nunca em pânico; e o que a validação aceita
    // o gerador tem de conseguir processar.
    let bytes = lang::to_ir(&synth(2)).unwrap_or_else(|d| panic!("{d}")).encode();
    let (mut aceitos, mut recusados) = (0, 0);
    for i in 0..bytes.len() {
        for f in [|b: u8| b ^ 0xFF, |b: u8| b.wrapping_add(1), |_| 0, |b: u8| b ^ 0x80] {
            let mut b = bytes.clone();
            b[i] = f(b[i]);
            if b == bytes {
                continue;
            }
            match Module::decode(&b).and_then(|m| ir::validate(&m, 1).map(|_| m)) {
                Ok(m) => {
                    aceitos += 1;
                    let _ = irgen::generate(&m, 1);
                }
                Err(_) => recusados += 1,
            }
        }
    }
    for n in 0..bytes.len() {
        assert!(Module::decode(&bytes[..n]).is_err(), "truncado em {n} foi aceito");
    }
    eprintln!("mutações de 1 byte: {recusados} recusadas, {aceitos} aceitas (ainda bem formadas)");
    assert!(recusados > 0 && aceitos > 0);
}

#[test]
fn contagens_corrompidas_nao_pedem_memoria_alem_do_arquivo() {
    // F14 (verificação 2026-11-17): nvalores e nargs de uma região, ambos
    // vindos do arquivo, pediam ~38 GB e abortavam o processo (SIGABRT, que
    // catch_unwind não pega). O limite tem de vir de bytes que existem.
    let mut m = lang::to_ir(&synth(3)).unwrap_or_else(|d| panic!("{d}"));
    let f = m.funcs[0];
    m.funcs[0].nvals = 900_000_000;
    m.regions[f.rfirst as usize + 1].nargs = 900_000_000;
    let m = Module::decode(&m.encode()).unwrap();
    let e = ir::validate(&m, 1).expect_err("contagem corrompida aceita");
    assert!(e.contains("nvals = 900000000 além dos"), "{e}");
}

#[test]
fn mutacoes_de_varios_bytes_nunca_derrubam_nem_pedem_memoria() {
    // A mutação de 1 byte não achou o F14; a verificação (q4, 2026-11-17)
    // achou com 1..4 bytes: 4,8% dos casos abortavam pedindo ~38 GB. Mesmo
    // gerador e semente do q4, sem filtro: um aborto derruba o processo.
    let q4: String = (0..3)
        .map(|k| {
            format!(
                "fn f{k}(n: word) -> word {{ let mut i = 0; let mut s = {k}; while i < n {{ match sign(i - 5) {{ -1 => {{ s = s + i; }} \
                 0 => {{ s = s * 2; }} _ => {{ if i % 3 == 0 {{ s = s - 1; }} else {{ s = s + 2; }} }} }} i = i + 1; }} return s; }}\n"
            )
        })
        .collect::<String>()
        + "fn main() -> word { return f0(3); }\n";
    let bytes = lang::to_ir(&q4).unwrap_or_else(|d| panic!("{d}")).encode();
    let mut s = 0xC0FFEEu64;
    let mut next = || {
        s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        s >> 11
    };
    let (mut aceitos, mut decodificados) = (0, 0);
    for _ in 0..100_000 {
        let mut b = bytes.clone();
        for _ in 0..1 + next() % 4 {
            let k = (next() % b.len() as u64) as usize;
            b[k] = next() as u8;
        }
        if let Ok(m) = Module::decode(&b) {
            decodificados += 1;
            if ir::validate(&m, 1).is_ok() {
                aceitos += 1;
                let _ = irgen::generate(&m, 1);
            }
        }
    }
    eprintln!("mutações de 1..4 bytes: {decodificados} decodificadas, {aceitos} aceitas");
    assert!(decodificados > aceitos && aceitos > 0);
}

#[test]
fn listas_e_nomes_ladrilham_extra_e_strings_em_ordem_canonica() {
    // Sem isto: lixo não referenciado e listas compartilhadas davam várias
    // codificações do mesmo módulo, e uma lista compartilhada por k
    // instruções multiplicava valores definíveis (e memória) por k.
    let ok = lang::to_ir(&synth(2)).unwrap_or_else(|d| panic!("{d}"));
    ir::validate(&ok, 1).unwrap();
    let mut m = ok.clone();
    m.extra.push(0);
    assert!(err(&m).contains("listas da função terminam em"), "{}", err(&m));
    let mut m = ok.clone();
    m.strings.push(b'x');
    assert!(err(&m).contains("strings: "), "{}", err(&m));
    // duas chamadas `f(x)` dividindo a mesma lista de argumentos
    let src = "fn f(a: word) -> word { return a; }\nfn main() -> word { let x = time(); return f(x) + f(x); }\n";
    let mut m = lang::to_ir(src).unwrap_or_else(|d| panic!("{d}"));
    let calls: Vec<usize> = (0..m.insts.len()).filter(|&k| m.insts[k].op == Op::Call).collect();
    assert_eq!(calls.len(), 2);
    let (b0, b1) = (m.insts[calls[0]].b as usize, m.insts[calls[1]].b);
    m.insts[calls[1]].b = b0 as u32;
    m.extra.remove(b1 as usize);
    for f in &mut m.funcs[1..] {
        f.sig -= (f.sig > b1) as u32;
    }
    for x in &mut m.insts {
        if x.b > b1 && matches!(x.op, Op::If | Op::Switch | Op::While | Op::Call | Op::Intr | Op::Yield) && x.b != 0 {
            x.b -= 1;
        }
    }
    assert!(err(&m).contains("a ordem canônica pede"), "{}", err(&m));
}

// ── módulos montados à mão ─────────────────────────────────────────────

fn i(op: Op, k: u8, n: u16, res: u32, a: u32, b: u32) -> Inst {
    Inst { op, k, n, res, a, b }
}

/// Uma função `main() -> word` com as regiões dadas (args, nargs, insts);
/// a primeira é a raiz. Deslocamentos de lista são relativos a `extra`; a
/// assinatura vem antes (ordem canônica), então eles andam 3.
fn module(regions: Vec<(u32, u32, Vec<Inst>)>, extra: Vec<u32>, nvals: u32) -> Module {
    let mut m = Module::default();
    let sig = 0;
    m.extra = [0, 1, 0].into_iter().chain(extra).collect();
    m.strings = b"main".to_vec();
    for (args, nargs, mut insts) in regions {
        for i in &mut insts {
            if matches!(i.op, Op::If | Op::Switch | Op::While) || matches!(i.op, Op::Call | Op::Intr | Op::Yield) && i.n > 0 {
                i.b += 3;
            }
        }
        m.regions.push(Region { args, nargs, first: m.insts.len() as u32, len: insts.len() as u32 });
        m.insts.extend(insts);
    }
    m.funcs.push(Func {
        name: 0,
        name_len: 4,
        sig,
        rfirst: 0,
        rcount: m.regions.len() as u32,
        ifirst: 0,
        icount: m.insts.len() as u32,
        nvals,
    });
    m
}

fn err(m: &Module) -> String {
    ir::validate(m, 1).expect_err("devia ser recusado")
}

const RET: Op = Op::Return;

#[test]
fn ponteiros_slot_ptradd_load_store_rodam_no_nucleo() {
    // p = slot 3; p[2] = 40; p[0] = 2; return p[2] + p[0]  → 42
    let m = module(
        vec![(
            0,
            0,
            vec![
                i(Op::Slot, 0, 0, 0, 3, 0),
                i(Op::Const, 0, 0, 1, 2, 0),
                i(Op::PtrAdd, 0, 0, 2, 0, 1),
                i(Op::Const, 0, 0, 3, 40, 0),
                i(Op::Store, 0, 0, 0, 2, 3),
                i(Op::Store, 0, 0, 0, 0, 1),
                i(Op::Load, 0, 0, 4, 2, 0),
                i(Op::Load, 0, 0, 5, 0, 0),
                i(Op::Bin, 0, 0, 6, 4, 5),
                i(RET, 0, 1, 0, 6, 0),
            ],
        )],
        vec![],
        7,
    );
    ir::validate(&m, 1).unwrap();
    assert!(ir::print(&m).contains("%2 = ptradd %0, %1"));
    let tas = irgen::generate(&m, 1).unwrap();
    let prog = tenerura_t27::asm::assemble_str(&tas, "tir", Some(&os_dir().join("user"))).unwrap();
    let mut s = boot_k(&kernel(), vec![task("p", prog).con()], 0, None).unwrap();
    let dead = s.k("ST_DEAD");
    run_checked(&mut s, 100_000, 64, |s| s.task("p").state == dead).unwrap();
    assert_eq!(s.task("p").exit, 42);
}

#[test]
fn ponteiro_e_palavra_nao_se_misturam() {
    // load de uma palavra
    let m = module(vec![(0, 0, vec![i(Op::Const, 0, 0, 0, 7, 0), i(Op::Load, 0, 0, 1, 0, 0), i(RET, 0, 1, 0, 1, 0)])], vec![], 2);
    assert!(err(&m).contains("%0 não é ptr"), "{}", err(&m));
    // aritmética com ponteiro (não há ptrtoint: a proveniência não se perde)
    let m = module(vec![(0, 0, vec![i(Op::Slot, 0, 0, 0, 1, 0), i(Op::Bin, 0, 0, 1, 0, 0), i(RET, 0, 1, 0, 1, 0)])], vec![], 2);
    assert!(err(&m).contains("%0 não é word"), "{}", err(&m));
}

#[test]
fn valor_de_um_ramo_nao_escapa_da_regiao() {
    // %1 = if %0 { %2 = const 5; yield } else { yield }; return %2  ← fora do escopo
    let m = module(
        vec![
            (0, 0, vec![i(Op::Const, 0, 0, 0, 1, 0), i(Op::If, 0, 0, 0, 0, 0), i(RET, 0, 1, 0, 1, 0)]),
            (0, 0, vec![i(Op::Const, 0, 0, 1, 5, 0), i(Op::Yield, 0, 0, 0, 0, 0)]),
            (0, 0, vec![i(Op::Yield, 0, 0, 0, 0, 0)]),
        ],
        vec![1, 2],
        2,
    );
    assert!(err(&m).contains("%1 usado fora do escopo"), "{}", err(&m));
}

#[test]
fn uso_antes_da_definicao_e_recusado() {
    let m = module(
        vec![(0, 0, vec![i(Op::Bin, 0, 0, 0, 1, 1), i(Op::Const, 0, 0, 1, 1, 0), i(RET, 0, 1, 0, 0, 0)])],
        vec![],
        2,
    );
    assert!(err(&m).contains("%1 usado fora do escopo"), "{}", err(&m));
}

#[test]
fn regiao_compartilhada_e_terminador_no_meio_sao_recusados() {
    // if com then == else: a árvore de regiões vira grafo
    let m = module(
        vec![
            (0, 0, vec![i(Op::Const, 0, 0, 0, 1, 0), i(Op::If, 0, 0, 0, 0, 0), i(RET, 0, 1, 0, 0, 0)]),
            (0, 0, vec![i(Op::Yield, 0, 0, 0, 0, 0)]),
        ],
        vec![1, 1],
        1,
    );
    assert!(err(&m).contains("usada duas vezes"), "{}", err(&m));
    let m = module(vec![(0, 0, vec![i(Op::Const, 0, 0, 0, 1, 0), i(RET, 0, 1, 0, 0, 0), i(RET, 0, 1, 0, 0, 0)])], vec![], 1);
    assert!(err(&m).contains("terminador fora do fim"), "{}", err(&m));
}

#[test]
fn terminador_errado_para_a_regiao_e_recusado() {
    // yield na raiz da função; cond fora de while; return na condição
    let m = module(vec![(0, 0, vec![i(Op::Yield, 0, 0, 0, 0, 0)])], vec![], 0);
    assert!(err(&m).contains("yield fora"), "{}", err(&m));
    let m = module(vec![(0, 0, vec![i(Op::Const, 0, 0, 0, 1, 0), i(Op::Cond, 0, 0, 0, 0, 0)])], vec![], 1);
    assert!(err(&m).contains("cond fora"), "{}", err(&m));
    // while () cond() { return %0 } body() { yield }
    let m = module(
        vec![
            (0, 0, vec![i(Op::Const, 0, 0, 0, 1, 0), i(Op::While, 0, 0, 0, 0, 0), i(RET, 0, 1, 0, 0, 0)]),
            (0, 0, vec![i(RET, 0, 1, 0, 0, 0)]),
            (0, 0, vec![i(Op::Yield, 0, 0, 0, 0, 0)]),
        ],
        vec![1, 2],
        1,
    );
    assert!(err(&m).contains("return na região de condição"), "{}", err(&m));
}

#[test]
fn codificacao_canonica_recusa_campos_sujos() {
    // campo não usado ≠ 0, preenchimento ≠ 0, constante fora de 27 trits
    let ok = module(vec![(0, 0, vec![i(Op::Const, 0, 0, 0, 1, 0), i(RET, 0, 1, 0, 0, 0)])], vec![], 1);
    ir::validate(&ok, 1).unwrap();
    let mut m = ok.clone();
    m.insts[1].b = 9;
    assert!(err(&m).contains("campo não usado"), "{}", err(&m));
    let mut m = ok.clone();
    m.insts[0].k = 1;
    assert!(err(&m).contains("campo não usado"), "{}", err(&m));
    let mut m = ok.clone();
    m.insts[0].b = 0x7FFF_FFFF;
    assert!(err(&m).contains("fora de 27 trits"), "{}", err(&m));
    let mut m = ok.clone();
    m.strings.push(b'x'); // "mainx": nome continua "main", mas o preenchimento muda
    let mut b = m.encode();
    *b.last_mut().unwrap() = 1;
    assert!(Module::decode(&b).unwrap_err().contains("preenchimento"));
}

#[test]
fn ramos_irmaos_dividem_as_palavras_do_quadro() {
    // 12 braços com 3 valores guardados cada: o quadro é o braço mais fundo
    // (docs/13 §C.2), não a soma — sem reaproveitar seriam 12 × 3 + 4
    let mut src = String::from("fn main() -> word {\n let x = time();\n let mut r = 0;\n match x % 13 {\n");
    for k in 0..12 {
        src += &format!("{k} => {{ let a = x * {k}; let b = a + x; let c = b * a; r = c - b; }}\n");
    }
    src += " _ => { }\n }\n return r;\n}\n";
    let tas = lang::compile(&src).unwrap_or_else(|d| panic!("{d}"));
    let frame: i64 = tas
        .lines()
        .skip_while(|l| !l.starts_with("fn_main:"))
        .find_map(|l| l.trim().strip_prefix("addi sp, sp, -").map(|n| n.parse().unwrap()))
        .expect("quadro de main");
    assert!(frame <= 8, "quadro de {frame} palavras: os braços não reaproveitam");
}
