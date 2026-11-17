#!/usr/bin/env python3
"""TENERURA — Fases 1-3 do tradutor LLVM IR -> assembly T27.

Fase 1: alloca | load | store | add | sub | call | ret.
Fase 2 acrescenta: mul | icmp | br (incondicional e condicional).
Fase 3 acrescenta: memória por ponteiro. alloca [N x T] (N slots seguidos),
  getelementptr (aritmética de endereço), load/store por ponteiro, sext/zext/trunc
  (cópia), globais constantes (strings/arrays -> `.data` com `.word`), ptr em
  parâmetros e argumentos. Modelo de memória: 1 palavra T27 = 1 elemento
  (i8, i32, i64 e ptr ocupam uma palavra cada); GEP escala em PALAVRAS.

Escopo deliberadamente mínimo: só o LLVM IR "ingênuo" que o `clang -O0`
emite para funções com variáveis locais escalares, laços e condicionais.

Suportado (nada além disto):
  alloca i32 | load i32 | store i32 | add | sub | mul
  icmp eq|ne|slt|sgt|sle|sge i32
  br label %L | br i1 %c, label %T, label %F | call i32 @f(...) | ret i32

Modelo mental (o mais burro que funciona):
  * TODA definição de valor LLVM (`%3 = ...`) vira UMA palavra na pilha
    do T27. Parâmetros também. Não há registradores virtuais, não há SSA,
    não há phi, não há mem2reg: o fluxo de dados passa SEMPRE pela memória.
  * Por isso `phi` não é preciso aqui: no `-O0` quem faz o papel do `phi`
    é um `alloca` reescrito por `store` em cada aresta do CFG. O laço do
    LLVM abaixo (`6: ... br label %6`) é exatamente isso: `%res` e `%b` são
    allocas do prólogo, não `phi`.
  * `icmp` é materializado como 0/1 num slot (`seq`/`sne` ou par
    `cmp`+`slt`), porque o enunciado pede booleano em slot e porque isso
    mantém o tradutor burro: o valor booleano tem endereço, como todo o resto.
  * `br i1` lê esse slot e desvia; não há otimização de "pular o slot".
  * A moldura é reservada no prólogo com `addi sp, sp, -N`; os slots são
    `0(sp)..(N-1)(sp)` — equivalentes a `-N(sp)..-1(sp)` do `sp` de entrada,
    e é o único jeito seguro (o `push` de quem é chamado desce para `-1(sp)`
    de quem chamou, então não dá para usar offsets negativos com `sp` parado).
  * `lr` é salvo no prólogo e restaurado no epílogo (toda função, sem análise).
  * Argumentos chegam em a1..a9 e são derramados na pilha no prólogo;
    o corpo só usa t1..t3.

Uso: ./tradutor.py teste.ll -o teste.tas
"""
from __future__ import annotations

import argparse
import re
import sys

ABI_INCLUDE = "/home/guto/tenerura/src/os/lib/abi.inc"


class ErroNaoSuportado(Exception):
    pass


# ── parsing ────────────────────────────────────────────────────────────────

RE_FUNC = re.compile(r"^define\b.*?@([\w.$]+)\s*\((.*?)\)\s*(?:#\d+)?\s*\{\s*$")
RE_LABEL = re.compile(r"^([\w.$]+):\s*$")
W = r"(?:i8|i32|i64|ptr)"
FLAGS = r"(?:(?:nuw|nsw)\s+)*"
RE_ALLOCA = re.compile(rf"^(%\S+)\s*=\s*alloca\s+(?:{W}|\[(\d+) x {W}\])")
RE_LOAD = re.compile(rf"^(%\S+)\s*=\s*load\s+{W},\s*ptr\s+([%@]\S+)")
RE_STORE = re.compile(rf"^store\s+{W}\s+(.+?),\s*ptr\s+([%@]\S+)")
RE_BINOP = re.compile(rf"^(%\S+)\s*=\s*(add|sub|mul)\s+{FLAGS}(?:i8|i32|i64)\s+(.+?),\s*(.+?)$")
RE_ICMP = re.compile(r"^(%\S+)\s*=\s*icmp\s+(eq|ne|slt|sgt|sle|sge)\s+(?:i8|i32|i64)\s+(.+?),\s*(.+?)$")
RE_CAST = re.compile(r"^(%\S+)\s*=\s*(?:sext|zext|trunc)\s+(?:nneg\s+)?(?:i1|i8|i32|i64)\s+(.+?)\s+to\s+(?:i8|i32|i64)$")
RE_GEP = re.compile(
    rf"^(%\S+)\s*=\s*getelementptr\s+(?:inbounds\s+)?{FLAGS}(\[\d+ x {W}\]|{W}),\s*ptr\s+([%@][^\s,]+)((?:,\s*i(?:32|64)\s+[^,]+)+)$")
RE_GLOBAL = re.compile(r"^@([\w.$]+)\s*=\s*(?:\w+\s+)*?(?:constant|global)\s+(.+)$")
# GEP como EXPRESSÃO CONSTANTE (`msg[1]` vira operando, não instrução). Só a forma
# tipada de dois índices; a forma em bytes (`i8, ptr @g, i64 K`) fica de fora de
# propósito: K seria em bytes e o modelo aqui é em palavras.
RE_GEP_CONST = re.compile(
    rf"getelementptr\s+(?:inbounds\s+)?{FLAGS}\(\[(\d+) x {W}\],\s*ptr\s+@([\w.$]+),"
    r"\s*i(?:32|64)\s+(-?\d+),\s*i(?:32|64)\s+(-?\d+)\)")
RE_BR = re.compile(r"^br\s+label\s+(%\S+)$")
RE_BRCOND = re.compile(r"^br\s+i1\s+(%\S+),\s*label\s+(%\S+),\s*label\s+(%\S+)$")
RE_CALL = re.compile(r"^(?:(%\S+)\s*=\s*)?call\s+(?:i32|void)\s+@([\w.$]+)\s*\((.*)\)\s*$")
RE_RET = re.compile(r"^ret\s+i32\s+(.+?)$")
RE_RET_VOID = re.compile(r"^ret\s+void$")

# `icmp` -> o par de desvios que testa o trit deixado por `cmp t3, t1, t2`
# (`cmp` dá o sinal de t1 − t2 em t3). Escolhemos sempre o caminho do
# "salta se a condição vale" para um rótulo que grava 1; caindo, grava 0.
#   seq: cmp t3,a,b ; beq t3,zr,L_verdade ; li t3,0 ; j L_fim ; L_verdade: li t3,1
ICMP_PAR = {
    "eq": ("beq", True), "ne": ("bne", True),
    "slt": ("blt", True), "sgt": ("bgt", True),
    "sle": ("ble", True), "sge": ("bge", True),
}


def limpar(linha: str) -> str:
    """Remove comentário e atributos que não mudam a semântica do subconjunto."""
    linha = linha.split(";", 1)[0].strip()
    linha = re.sub(r",\s*align\s+\d+", "", linha)
    linha = re.sub(r"\bnoundef\s+", "", linha)
    # metadados de laço (`br label %6, !llvm.loop !6`) não mudam nada aqui
    linha = re.sub(r",\s*!llvm\.[\w.]+\s+![\w.\d]+", "", linha)
    # GEP constante -> operando `@global+deslocamento` (em palavras)
    linha = RE_GEP_CONST.sub(lambda m: f"@{m[2]}+{int(m[3]) * int(m[1]) + int(m[4])}", linha)
    return linha


TIPOS_PALAVRA = ("i8", "i32", "i64", "ptr")


def operando(txt: str) -> str:
    """'%3' ou '42' ou '-7' -> nome ou literal, já normalizado."""
    txt = txt.strip()
    if txt.startswith(("%", "@")):
        return txt
    try:
        int(txt)
    except ValueError:
        raise ErroNaoSuportado(f"operando não suportado: {txt!r}")
    return txt


def nome_label(txt: str) -> str:
    """'%if.then' -> 'if.then'; '6' -> '6'."""
    return txt[1:] if txt.startswith("%") else txt


def parametros(txt: str) -> list[str]:
    txt = txt.strip()
    if not txt:
        return []
    out = []
    for p in txt.split(","):
        partes = p.split()
        if not partes:
            continue
        if partes[0] not in TIPOS_PALAVRA:
            raise ErroNaoSuportado(f"parâmetro de tipo não suportado: {p.strip()!r}")
        nome = partes[-1]
        if not nome.startswith("%"):
            raise ErroNaoSuportado(f"parâmetro sem nome: {p.strip()!r}")
        out.append(nome)
    return out


def decodificar_c(txt: str) -> list[int]:
    """Conteúdo de c"..." do LLVM (escapes \\XX em hexa) -> uma palavra por byte."""
    out, i = [], 0
    while i < len(txt):
        if txt[i] == "\\":
            out.append(int(txt[i + 1:i + 3], 16))
            i += 3
        else:
            out.append(ord(txt[i]))
            i += 1
    return out


def parse_global(n: int, resto: str) -> list[int]:
    """`[6 x i8] c"Hello\\00"` | `[3 x i32] [i32 1, i32 2, i32 3]` | `... zeroinitializer` | `i32 7`."""
    m = re.match(rf"^\[(\d+) x {W}\]\s+(.+)$", resto)
    if m:
        qtd, ini = int(m.group(1)), m.group(2).strip()
        if ini == "zeroinitializer":
            vals = [0] * qtd
        elif ini.startswith('c"') and ini.endswith('"'):
            vals = decodificar_c(ini[2:-1])
        elif ini.startswith("[") and ini.endswith("]"):
            vals = [int(x.split()[-1]) for x in ini[1:-1].split(",") if x.strip()]
        else:
            raise ErroNaoSuportado(f"linha {n}: inicializador de global não suportado: {ini!r}")
        if len(vals) != qtd:
            raise ErroNaoSuportado(f"linha {n}: global com {len(vals)} valores para {qtd} posições")
        return vals
    m = re.match(rf"^{W}\s+(-?\d+|zeroinitializer)$", resto)
    if m:
        return [0 if m.group(1) == "zeroinitializer" else int(m.group(1))]
    raise ErroNaoSuportado(f"linha {n}: global não suportada: {resto!r}")


def rotulo_global(nome: str) -> str:
    return "G_" + re.sub(r"[^\w]", "_", nome)


def parse(texto: str) -> tuple[list[dict], dict[str, list[int]]]:
    globais: dict[str, list[int]] = {}
    funcs: list[dict] = []
    atual = None
    for n, bruta in enumerate(texto.splitlines(), 1):
        linha = limpar(bruta)
        if not linha:
            continue
        if atual is None:
            mg = RE_GLOBAL.match(linha)
            if mg:
                globais[mg.group(1)] = parse_global(n, mg.group(2))
                continue
            m = RE_FUNC.match(linha)
            if m:
                atual = {
                    "nome": m.group(1),
                    "params": parametros(m.group(2)),
                    "instrs": [],
                    "linha": n,
                }
            continue
        if linha == "}":
            funcs.append(atual)
            atual = None
            continue
        if linha.startswith(("declare ", "attributes ", "!", "source_filename", "target ", "define ")):
            raise ErroNaoSuportado(f"linha {n}: construção de módulo não suportada: {linha!r}")
        atual["instrs"].append((n, linha))
    if atual is not None:
        raise ErroNaoSuportado("função sem '}' final")
    return funcs, globais


# ── geração ────────────────────────────────────────────────────────────────

def classificar(n: int, linha: str) -> tuple[str, tuple]:
    m = RE_LABEL.match(linha)
    if m:
        return "label", (nome_label(m.group(1)),)
    m = RE_ALLOCA.match(linha)
    if m:
        return "alloca", (m.group(1), int(m.group(2)) if m.group(2) else 1)
    m = RE_LOAD.match(linha)
    if m:
        return "load", (m.group(1), m.group(2))
    m = RE_STORE.match(linha)
    if m:
        return "store", (operando(m.group(1)), m.group(2))
    m = RE_BINOP.match(linha)
    if m:
        return m.group(2), (m.group(1), operando(m.group(3)), operando(m.group(4)))
    m = RE_CAST.match(linha)
    if m:
        return "cast", (m.group(1), operando(m.group(2)))
    m = RE_GEP.match(linha)
    if m:
        idx = [operando(x.split(None, 1)[1]) for x in m.group(4).split(",")[1:]]
        return "gep", (m.group(1), m.group(2), m.group(3), idx)
    m = RE_ICMP.match(linha)
    if m:
        return "icmp", (m.group(1), m.group(2), operando(m.group(3)), operando(m.group(4)))
    m = RE_BRCOND.match(linha)
    if m:
        return "br_cond", (m.group(1), nome_label(m.group(2)), nome_label(m.group(3)))
    m = RE_BR.match(linha)
    if m:
        return "br", (nome_label(m.group(1)),)
    m = RE_CALL.match(linha)
    if m:
        args = []
        for bruto in m.group(3).split(","):
            bruto = bruto.strip()
            if not bruto:
                continue
            partes = bruto.split()
            if len(partes) > 1:
                if partes[0] not in TIPOS_PALAVRA:
                    raise ErroNaoSuportado(f"linha {n}: argumento de tipo não suportado: {bruto!r}")
                bruto = partes[-1]
            args.append(operando(bruto))
        return "call", (m.group(1), m.group(2), args)
    if RE_RET_VOID.match(linha):
        return "ret_void", ()
    m = RE_RET.match(linha)
    if m:
        return "ret", (operando(m.group(1)),)
    raise ErroNaoSuportado(f"linha {n}: instrução fora do subconjunto: {linha!r}")


# operações que definem um valor LLVM e portanto precisam de um slot
OPS_COM_DESTINO = ("load", "add", "sub", "mul", "icmp", "call", "cast", "gep")


def slots_da_funcao(f: dict) -> tuple[dict[str, int], dict[str, str], int]:
    """Uma palavra por valor LLVM (parâmetros, temporários); `alloca [N x T]` leva N
    palavras seguidas. `tipo[nome]` é 'alloca' (o nome É o endereço sp+slot) ou 'val'."""
    slots: dict[str, int] = {}
    tipo: dict[str, str] = {}
    total = 0
    for p in f["params"]:
        slots[p], tipo[p] = total, "val"
        total += 1
    for n, linha in f["instrs"]:
        op, args = classificar(n, linha)
        destino = args[0] if op in OPS_COM_DESTINO or op == "alloca" else None
        if destino and destino not in slots:
            slots[destino] = total
            if op == "alloca":
                tipo[destino] = "alloca"
                total += args[1]
            else:
                tipo[destino] = "val"
                total += 1
    return slots, tipo, total


def carregar(valor: str, ctx, reg: str) -> list[str]:
    """Põe um operando em `reg`: literal -> li; @global -> endereço (la);
    nome de alloca -> endereço sp+slot; demais valores -> ld do slot."""
    slots, tipo, globais = ctx
    if valor.startswith("%"):
        if tipo[valor] == "alloca":
            return [f"    addi {reg}, sp, {slots[valor]}"]
        return [f"    ld {reg}, {slots[valor]}(sp)"]
    if valor.startswith("@"):
        nome, _, desloc = valor[1:].partition("+")   # `@g` ou `@g+K` (GEP constante)
        if nome not in globais:
            raise ErroNaoSuportado(f"global desconhecida: {valor}")
        return [f"    la {reg}, {rotulo_global(nome)}" + (f" + {desloc}" if desloc else "")]
    return [f"    li {reg}, {valor}"]


def rotulo(nome: str) -> str:
    """'%if.then' / '6' -> 'L6'. Prefixo por função: rótulos são globais."""
    return "L_" + nome


def gerar(f: dict, globais: dict[str, list[int]]) -> list[str]:
    slots, tipo, total = slots_da_funcao(f)
    ctx = (slots, tipo, globais)
    rotulos = {
        nome for _, (op, args) in ((l, classificar(*l)) for l in f["instrs"])
        if op == "label" for nome in args
    }
    lr_slot = total
    moldura = lr_slot + 1
    out = [f"{f['nome']}:"]
    contador = [0]  # rótulos sintéticos para o `icmp`

    def novo_rotulo() -> str:
        contador[0] += 1
        return f"L__icmp_{f['nome']}_{contador[0]}"

    def desvio(cond: str, alvo: str) -> str:
        if alvo not in rotulos:
            raise ErroNaoSuportado(f"{f['nome']}: desvio para rótulo inexistente '{alvo}'")
        if cond == "j":
            return f"    j {rotulo(f['nome'] + '__' + alvo)}"
        return f"    {cond} t1, {rotulo(f['nome'] + '__' + alvo)}"

    # prólogo: moldura, lr, derramamento dos argumentos
    out.append(f"    addi sp, sp, -{moldura}")
    out.append(f"    st lr, {lr_slot}(sp)")
    for i, p in enumerate(f["params"]):
        out.append(f"    st a{i + 1}, {slots[p]}(sp)")

    for n, linha in f["instrs"]:
        op, args = classificar(n, linha)
        if op == "label":
            out.append(f"{rotulo(f['nome'] + '__' + args[0])}:")
        elif op == "alloca":
            continue
        elif op == "store":
            valor, ptr = args
            out += carregar(valor, ctx, "t1")
            if tipo.get(ptr) == "alloca":          # acesso direto ao slot
                out.append(f"    st t1, {slots[ptr]}(sp)")
            else:                                  # por ponteiro: endereço em t2
                out += carregar(ptr, ctx, "t2")
                out.append("    st t1, 0(t2)")
        elif op == "load":
            destino, ptr = args
            if tipo.get(ptr) == "alloca":
                out.append(f"    ld t1, {slots[ptr]}(sp)")
            else:
                out += carregar(ptr, ctx, "t1")
                out.append("    ld t1, 0(t1)")
            out.append(f"    st t1, {slots[destino]}(sp)")
        elif op == "cast":                         # sext/zext/trunc: palavra é larga, é cópia
            destino, origem = args
            out += carregar(origem, ctx, "t1")
            out.append(f"    st t1, {slots[destino]}(sp)")
        elif op == "gep":
            destino, base_t, base, idx = args
            escalas = [int(re.match(r"\[(\d+)", base_t).group(1)), 1] if base_t.startswith("[") else [1]
            if len(idx) > len(escalas):
                raise ErroNaoSuportado(f"linha {n}: GEP com índices demais para {base_t}")
            out += carregar(base, ctx, "t1")       # endereço-base em t1 (em palavras)
            for ix, esc in zip(idx, escalas):
                if not ix.startswith("%"):
                    if int(ix) * esc:
                        out.append(f"    addi t1, t1, {int(ix) * esc}")
                    continue
                out += carregar(ix, ctx, "t2")
                if esc != 1:
                    out.append(f"    li t3, {esc}")
                    out.append("    mul t2, t2, t3")
                out.append("    add t1, t1, t2")
            out.append(f"    st t1, {slots[destino]}(sp)")
        elif op in ("add", "sub", "mul"):
            destino, esq, dir_ = args
            out += carregar(esq, ctx, "t1")
            out += carregar(dir_, ctx, "t2")
            out.append(f"    {op} t3, t1, t2")
            out.append(f"    st t3, {slots[destino]}(sp)")
        elif op == "icmp":
            destino, cond, esq, dir_ = args
            if cond not in ICMP_PAR:
                raise ErroNaoSuportado(f"linha {n}: condição icmp '{cond}' não suportada")
            salto, _ = ICMP_PAR[cond]
            # `cmp` deixa o sinal de (esq − dir_) em t3; o desvio de dois
            # registradores compara t3 com zr. Condição VERDADEIRA => salta
            # para o rótulo que grava 1; senão cai e grava 0.
            out += carregar(esq, ctx, "t1")
            out += carregar(dir_, ctx, "t2")
            out.append("    cmp t3, t1, t2")
            verdade, fim = novo_rotulo(), novo_rotulo()
            out.append(f"    {salto} t3, zr, {verdade}")
            out.append("    li t3, 0")
            out.append(f"    j {fim}")
            out.append(f"{verdade}:")
            out.append("    li t3, 1")
            out.append(f"{fim}:")
            out.append(f"    st t3, {slots[destino]}(sp)")
        elif op == "br":
            out.append(desvio("j", args[0]))
        elif op == "br_cond":
            cond_v, alvo_v, alvo_f = args
            if not cond_v.startswith("%"):
                raise ErroNaoSuportado(f"linha {n}: br i1 com operando não-SSA: {cond_v!r}")
            out += carregar(cond_v, ctx, "t1")
            out.append(desvio("bnz", alvo_v))
            out.append(desvio("j", alvo_f))
        elif op == "call":
            destino, nome, argumentos = args
            if len(argumentos) > 9:
                raise ErroNaoSuportado(f"linha {n}: mais de 9 argumentos")
            for i, a in enumerate(argumentos):
                out += carregar(a, ctx, f"a{i + 1}")
            out.append(f"    call {nome}")
            if destino:
                out.append(f"    st a1, {slots[destino]}(sp)")
        elif op == "ret":
            (valor,) = args
            out += carregar(valor, ctx, "a1")
            out.append(f"    ld lr, {lr_slot}(sp)")
            out.append(f"    addi sp, sp, {moldura}")
            out.append("    ret")
        elif op == "ret_void":
            out.append(f"    ld lr, {lr_slot}(sp)")
            out.append(f"    addi sp, sp, {moldura}")
            out.append("    ret")
    out.append("")
    return out


def main() -> int:
    ap = argparse.ArgumentParser(description="LLVM IR (-O0) -> assembly T27")
    ap.add_argument("ir")
    ap.add_argument("-o", "--saida", default=None)
    a = ap.parse_args()

    texto = open(a.ir, encoding="utf-8").read()
    funcs, globais = parse(texto)
    if not funcs:
        print("erro: nenhuma função encontrada", file=sys.stderr)
        return 1

    linhas = [
        "; ============================================================",
        "; GERADO POR tradutor.py — NÃO EDITE À MÃO",
        f"; fonte: {a.ir}",
        "; convenção: uma palavra de pilha por valor LLVM; sp cresce para baixo",
        "; Fase 2: + mul, icmp, br (if/else e while)",
        "; Fase 3: + alloca [N x T], GEP, load/store por ponteiro, globais",
        "; ============================================================",
        f'    .include "{ABI_INCLUDE}"',
        "    .org USER_TEXT",
        "_start:",
        "    call main",
        "    mov a2, a1                  ; código de saída = valor de retorno de main",
        "    li a1, SYS_EXIT",
        "    sys",
        "",
    ]
    for f in funcs:
        linhas += gerar(f, globais)
    if globais:
        linhas += ["    .data"]
        for nome, vals in globais.items():
            linhas.append(f"{rotulo_global(nome)}:")
            for k in range(0, len(vals), 8):
                linhas.append("    .word " + ", ".join(str(v) for v in vals[k:k + 8]))
        linhas.append("")

    saida = a.saida or a.ir.rsplit(".", 1)[0] + ".tas"
    with open(saida, "w", encoding="utf-8") as fh:
        fh.write("\n".join(linhas) + "\n")
    print(f"{saida}: {len(funcs)} função(ões), {len(linhas)} linhas")
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except ErroNaoSuportado as e:
        print(f"erro: {e}", file=sys.stderr)
        sys.exit(1)