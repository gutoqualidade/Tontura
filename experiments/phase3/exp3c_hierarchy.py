#!/usr/bin/env python3
# TENERURA Fase 3C — hierarquia numérica (§4 do plano)
# Partes:
#   C1. Tabela de famílias: fatoração, nº de divisores, aninhamento base 3
#       (resíduos ao agrupar de 3 em 3), primalidade.
#   C2. Árvores de composição para totais fixos (27, 81, 243, 729, 15, 21, 63):
#       todas as fatorações ordenadas em fatores ímpares >1; métricas:
#       profundidade, uniformidade, todos-ímpares (⇒ faixa balanceada em cada
#       nível), nº de radizes distintas no endereçamento aninhado.
#   C3. Eficiência de empacotamento binário: bits = ceil(n·log2 3) por n,
#       desperdício, melhor n por largura de contêiner (8/16/32/64/128 bits)
#       e convergentes de log2 3 (fração contínua).
# Determinístico; sem sementes (nada aleatório). Limitação: métricas são
# estruturais (combinatórias), não benchmarks de desempenho.
import csv
import math
from itertools import combinations_with_replacement

LOG2_3 = math.log2(3)


def factorize(n):
    fs = []
    d = 3
    while d * d <= n:
        while n % d == 0:
            fs.append(d)
            n //= d
        d += 2
    if n > 1:
        fs.append(n)
    return fs


def num_divisors(n):
    fs = {}
    for f in factorize(n):
        fs[f] = fs.get(f, 0) + 1
    r = 1
    for e in fs.values():
        r *= e + 1
    return r


def is_prime(n):
    return n > 1 and len(factorize(n)) == 1


def nesting_residue(n, g):
    """Estados residuais ao organizar n elementos em grupos de g."""
    return n % g


# C1 ------------------------------------------------------------------
FAMILIES = {
    "pot3": [3, 9, 27, 81, 243, 729, 2187],
    "impares_consecutivos": [3, 5, 7, 9, 11, 13, 15, 17, 19, 21, 27],
    "primos_impares": [3, 5, 7, 11, 13, 17, 19, 23, 29, 31],
    "mersenne": [3, 7, 31, 127, 511, 2047, 8191],
    "compostos": [9, 15, 21, 27, 33, 45, 63, 81],
}


def c1_families():
    rows = []
    for fam, ns in FAMILIES.items():
        for n in ns:
            fs = factorize(n)
            f3 = fs.count(3)
            rows.append((
                fam, n, "*".join(map(str, fs)), len(fs), num_divisors(n),
                is_prime(n), f3, n % 3, n % 9, n % 27,
                nesting_residue(n, 3), nesting_residue(n, 5), nesting_residue(n, 7),
                n % 2,
            ))
    return rows


# C2 ------------------------------------------------------------------
def odd_factor_trees(n, min_factor=3):
    """Todas as decomposições MULTI-conjunto de n em fatores ímpares >1
    (ordem irrelevante; representada como tupla ordenada). [n] = sem grupo."""
    if n == 1:
        return {()}
    results = set()
    for d in range(min_factor, n + 1, 2):
        if n % d == 0:
            for rest in odd_factor_trees(n // d, d):  # não-decrescente
                results.add(tuple(sorted((d,) + rest)))
    results.add((n,))  # átomo (sem decompor)
    return results


def c2_trees():
    rows = []
    for T in (27, 81, 243, 729, 15, 21, 63, 5, 7, 11):
        for tree in sorted(odd_factor_trees(T)):
            depth = len(tree)
            uniform = len(set(tree)) == 1
            all_odd = all(f % 2 == 1 for f in tree)
            distinct_radices = len(set(tree))
            # custo de endereçamento aninhado: nº de dígitos = profundidade;
            # endereçamento puro base-3 ⟺ todos fatores = 3
            pure_ternary = all(f == 3 for f in tree)
            # resíduo em cada nível: decomposição exata por construção
            rows.append((T, "*".join(map(str, tree)) if tree else "1",
                         depth, uniform, all_odd, distinct_radices, pure_ternary))
    return rows


# C3 ------------------------------------------------------------------
def continued_fraction(x, k=8):
    cf = []
    for _ in range(k):
        a = int(x)
        cf.append(a)
        x = 1 / (x - a)
    return cf


def convergents(cf):
    out = []
    p_n, p_d, p_p, p_p2 = cf[0], 1, 1, 0
    q_n, q_d, q_p, q_p2 = 1, 0, 0, 1
    out.append((p_n, q_n))
    for a in cf[1:]:
        p_n, p_p, p_p2 = a * p_n + p_p, p_n, p_p
        q_n, q_p, q_p2 = a * q_n + q_p, q_n, q_p
        out.append((p_n, q_n))
    return out  # pares (bits, trits) aproximando log2 3


def c3_packing():
    rows = []
    best_per_width = {}
    for n in range(1, 82):
        bits = math.ceil(n * LOG2_3)
        waste = bits - n * LOG2_3
        width = 8
        while width < bits:
            width *= 2
        eff = n * LOG2_3 / bits
        rows.append((n, bits, round(waste, 4), width, round(eff, 4)))
        # melhor n por largura: mais trits que couber
        key = width
        if key not in best_per_width or n > best_per_width[key][0]:
            best_per_width[key] = (n, bits, round(waste, 4))
    # convergentes de log2 3 (aproximações p/q ~ bits/trits)
    cf = continued_fraction(LOG2_3)
    convs = [(q, p, abs(p / q - LOG2_3)) for p, q in convergents(cf)]
    return rows, best_per_width, convs


if __name__ == "__main__":
    import os
    os.makedirs("results/phase3", exist_ok=True)

    print("== C1 famílias (fam, n, fatoração, nºfat, nºdiv, primo, pot3-exact, n%3, n%9, n%27, res%3, res%5, res%7, paridade) ==")
    c1 = c1_families()
    with open("results/phase3/exp3c_c1_familias.csv", "w", newline="") as f:
        w = csv.writer(f)
        w.writerow(["familia", "n", "fatoracao", "num_fatores", "num_divisores",
                    "primo", "e_potencia_de_3", "n_mod_3", "n_mod_9", "n_mod_27",
                    "residuo_grupos3", "residuo_grupos5", "residuo_grupos7", "paridade"])
        w.writerows(c1)
    for r in c1:
        print(r)

    print("\n== C2 árvores (T, árvore, profundidade, uniforme, todos_ímpares, radizes_distintas, puro_base3) ==")
    c2 = c2_trees()
    with open("results/phase3/exp3c_c2_arvores.csv", "w", newline="") as f:
        w = csv.writer(f)
        w.writerow(["T", "arvore", "profundidade", "uniforme", "todos_impares",
                    "radizes_distintas", "puro_base3"])
        w.writerows(c2)
    for r in c2:
        print(r)

    print("\n== C3 empacotamento: melhores n por largura (largura: n, bits, desperdício) ==")
    c3, best, convs = c3_packing()
    with open("results/phase3/exp3c_c3_packing.csv", "w", newline="") as f:
        w = csv.writer(f)
        w.writerow(["n", "bits_necessarios", "desperdicio_bits", "contaier_largura", "eficiencia"])
        w.writerows(c3)
    for width, (n, bits, waste) in sorted(best.items()):
        print(f"  {width:3d} bits -> n={n:2d} ({bits} bits usados, desperdício {waste:.3f})")
    print("  convergentes de log2 3 (trits, bits, erro):")
    for q, p, err in convs:
        print(f"    n={q:3d} bits={p:4d} erro={err:.5f}")
    # melhor n por largura com desperdício mínimo também (alternativa)
    print("  (n por largura = máximo que cabe; desperdício mínimo é outro critério)")
