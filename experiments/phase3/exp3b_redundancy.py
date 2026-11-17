#!/usr/bin/env python3
# TENERURA Fase 3B — redundância ímpar e estabilidade (§3 do plano)
# Experimento único com três partes:
#   P1. Códigos de repetição (r ∈ {1..9,11,13,2,4,6,8}): verificação EXAUSTIVA
#       da capacidade t = floor((r-1)/2) por substituição e do orçamento
#       2e+s ≤ r−1 (e substituições + s apagamentos).
#       Argumento de completude: para palavra canônica [s]*r, o resultado da
#       maioria depende ONLY da composição do multiset de votos (nº de votos
#       errados e, apagados s, corretos r−s−e) — posições são irrelevantes.
#       Pior caso: os e votos errados concentrados num único símbolo oposto.
#       Verificamos TODAS as composições (e, s, divisão dos erros) — exaustivo.
#       Pares usam regra de empate → apagamento (decodificação falha, não erra).
#   P2. Monte Carlo: 4 modelos de ruído × r ∈ {1,3,5,7,9,11,13,4,6,8} ×
#       p ∈ {1e-3,3e-3,1e-2,3e-2,1e-1}; palavra enviada sempre canônica all-+1
#       (simetria de símbolos estabelecida em P1); IC de Wilson 95%.
#       Sementes determinísticas derivadas de crc32 (hash() de str é
#       randomizado por processo — não reprodutível; corrigido).
#   P3. Níveis multiníveis {3,5,7,9}: probabilidades EXATAS (Gaussiana) de
#       acerto / apagamento (banda de guarda no nível intermediário) /
#       substituição não detectada.
# Semente base: SEED_MC = 0x5EED3B01 (fixa; derivações por crc32, sem escolha
# por resultado). Saídas: results/phase3/exp3b_{p1,p2,p3}.csv.
# Limitações: simulação estatística ≠ circuito físico; amplitude abstrata;
# correlação modelada apenas localmente (rajadas de 3 componentes).
import csv
import math
import zlib

import numpy as np
from scipy.stats import norm

SEED_MC = 0x5EED3B01
SYMS = (-1, 0, 1)


def majority(votes):
    """Maioria sobre votos ∈ {−1,0,+1, None(=apagado)}. Empate → None."""
    c = {s: 0 for s in SYMS}
    for v in votes:
        if v is not None:
            c[v] += 1
    best = max(c.values())
    winners = [s for s in SYMS if c[s] == best]
    return winners[0] if len(winners) == 1 else None


# ---------------- P1: verificação exaustiva por composições ----------------
def exhaustive_repetition():
    """Para cada r: (a) correção de e ≤ t substituições (s=0); (b) primeira
    falha com e ≥ 1; (c) orçamento 2e+s ≤ r−1 ⇒ decodificação correta;
    (d) nº de composições com empate exato (todas devem → apagamento)."""
    rows = []
    for r in (1, 2, 3, 4, 5, 6, 7, 8, 9, 11, 13):
        t = (r - 1) // 2
        ok_correction = True
        first_fail = None
        ok_budget = True
        n_ties = 0
        ties_all_erasure = True
        for sent_idx in range(3):
            s_sent = SYMS[sent_idx]
            others_idx = [i for i in range(3) if i != sent_idx]
            # todas as composições: e erros (0..r) divididos entre os 2
            # símbolos opostos; s apagamentos (0..r−e).
            for e in range(0, r + 1):
                for w1 in range(0, e + 1):  # erros no 1º símbolo oposto
                    w2 = e - w1
                    for s_era in range(0, r - e + 1):
                        counts = [0, 0, 0]
                        counts[sent_idx] = r - e - s_era
                        counts[others_idx[0]] = w1
                        counts[others_idx[1]] = w2
                        best = max(counts)
                        winners = [i for i, c in enumerate(counts) if c == best]
                        if len(winners) == 1:
                            dec = SYMS[winners[0]]
                        else:
                            dec = None  # regra: empate → apagamento
                            if sum(counts) > 0:
                                n_ties += 1
                        if e <= t and s_era == 0 and dec != s_sent:
                            ok_correction = False
                        if e >= 1 and first_fail is None and dec != s_sent:
                            first_fail = (e, s_era)
                        if 2 * e + s_era <= r - 1 and dec != s_sent:
                            ok_budget = False
        rows.append((r, t, ok_correction, str(first_fail), ok_budget, n_ties))
    return rows


# ---------------- P2: Monte Carlo ----------------
def wilson(k, n, z=1.96):
    if n == 0:
        return (0.0, 1.0)
    p = k / n
    d = 1 + z * z / n
    c = (p + z * z / (2 * n)) / d
    rr = z * math.sqrt(p * (1 - p) / n + z * z / (4 * n * n)) / d
    return (max(0.0, c - rr), min(1.0, c + rr))


def decode_counts(cm, c0, c1):
    """Decodificação vetorial: única maioria == +1 → correto; senão erro
    (substituição ou empate→apagamento: ambos são falha do código)."""
    counts = np.stack([cm, c0, c1], axis=1)
    best = counts.max(axis=1)
    unique = (counts == best[:, None]).sum(axis=1) == 1
    return unique & (counts[:, 2] == best)


def simulate(model, r, p, n_words=100_000, seed=SEED_MC):
    rng = np.random.default_rng(seed)
    syms = np.array([-1, 0, 1])
    # palavra enviada: canônica all-+1 (simetria de símbolos — P1)
    votes = np.ones((n_words, r), dtype=np.int8)
    if model == "indep":
        flip = rng.random((n_words, r)) < p
        newval = syms[rng.integers(0, 3, size=(n_words, r))]
        votes = np.where(flip, newval, votes)
    elif model == "burst":
        burst = rng.random((n_words, 1)) < p / 3.0
        start = rng.integers(0, max(1, r - 2), size=(n_words, 1))
        hit = np.zeros((n_words, r), dtype=bool)
        for off in range(3):
            idx = np.clip(start + off, 0, r - 1)[:, 0]
            hit[np.arange(n_words), idx] |= burst[:, 0]
        flip = hit & (rng.random((n_words, r)) < 0.5)
        flip |= (rng.random((n_words, r)) < p) & ~hit  # ruído de fundo
        newval = syms[rng.integers(0, 3, size=(n_words, r))]
        votes = np.where(flip, newval, votes)
    elif model == "asym":
        to0 = rng.random((n_words, r)) < p / 2        # → 0 mais provável
        toother = (rng.random((n_words, r)) < p / 4) & ~to0
        other = syms[rng.integers(0, 2, size=(n_words, r))] * 2 - 1  # ±1
        votes = np.where(to0, np.int8(0), votes)
        votes = np.where(toother, other, votes)
    elif model == "erasure":
        erased = rng.random((n_words, r)) < p
        votes = np.where(erased, np.int8(2), votes)  # 2 = apagado
    else:
        raise ValueError(model)
    cm = (votes == -1).sum(axis=1)
    c0 = (votes == 0).sum(axis=1)
    c1 = (votes == 1).sum(axis=1)
    if model == "erasure":
        ok = decode_counts(cm, c0, c1)
    else:
        ok = decode_counts(cm, c0, c1)
    return int((~ok).sum()), n_words


def mc_table():
    rows = []
    for model in ("indep", "burst", "asym", "erasure"):
        for r in (1, 3, 5, 7, 9, 11, 13, 4, 6, 8):
            for p in (1e-3, 3e-3, 1e-2, 3e-2, 1e-1):
                # semente determinística (crc32 — reprodutível entre processos)
                tag = f"{model}|r={r}|p={p}"
                seed = (SEED_MC + zlib.crc32(tag.encode())) & 0xFFFFFFFF
                k, n = simulate(model, r, p, seed=seed)
                lo, hi = wilson(k, n)
                rows.append((model, r, p, k, n, k / n, lo, hi, seed))
    return rows


# ---------------- P3: níveis multiníveis (exato) ----------------
def multilevel():
    """L níveis físicos em {−h..h}, h=(L−1)/2. Lógicos = {−h, 0, h} (extremos
    e centro); demais = guarda (leitura lá → apagamento). Ruído gaussiano de
    amplitude N(0,σ); decisão = nível mais próximo (fronteiras em meios-inteiros).
    Probabilidades EXATAS via normal."""
    out = []
    for L in (3, 5, 7, 9):
        h = (L - 1) // 2
        levels = list(range(-h, h + 1))
        logical = {-h, 0, h}
        for sigma in (0.1, 0.25, 0.5, 0.75, 1.0, 1.5):
            p_ok = p_guard = p_sub = 0.0
            for sent in (-h, 0, h):
                for lvl in levels:
                    if lvl == -h:
                        prob = norm.cdf((-h + 0.5 - sent) / sigma)
                    elif lvl == h:
                        prob = 1.0 - norm.cdf((h - 0.5 - sent) / sigma)
                    else:
                        prob = norm.cdf((lvl + 0.5 - sent) / sigma) - norm.cdf(
                            (lvl - 0.5 - sent) / sigma)
                    if lvl == sent:
                        p_ok += prob / 3
                    elif lvl in logical:
                        p_sub += prob / 3  # confundido com OUTRO lógico
                    else:
                        p_guard += prob / 3  # nível de guarda → apagamento
            out.append((L, sigma, p_ok, p_guard, p_sub))
    return out


if __name__ == "__main__":
    import os
    os.makedirs("results/phase3", exist_ok=True)

    print("== P1 exaustivo (r, t, correção e≤t s=0, 1ª falha (e,s) e≥1, orçamento 2e+s≤r−1, n_empates) ==")
    p1 = exhaustive_repetition()
    with open("results/phase3/exp3b_p1.csv", "w", newline="") as f:
        w = csv.writer(f)
        w.writerow(["r", "t", "corrige_ate_t", "primeira_falha_e_s", "orcamento_2e+s_le_r-1", "n_casos_empate"])
        w.writerows(p1)
    for row in p1:
        print(row)

    print("== P2 Monte Carlo ==")
    p2 = mc_table()
    with open("results/phase3/exp3b_p2.csv", "w", newline="") as f:
        w = csv.writer(f)
        w.writerow(["modelo", "r", "p", "falhas", "n", "taxa_erro", "ic95_lo", "ic95_hi", "seed"])
        w.writerows(p2)
    print(f"total configs: {len(p2)}")
    for model in ("indep", "burst", "asym", "erasure"):
        sel = [x for x in p2 if x[0] == model and x[2] in (1e-2, 1e-1) and x[1] in (1, 3, 5, 7, 9)]
        for m, r, p, k, n, e, lo, hi, s in sel:
            print(f"{model:8s} r={r:2d} p={p:.3f} err={e:.5f} IC95=[{lo:.5f},{hi:.5f}] seed={s}")

    print("== P3 multiníveis (L, sigma, P_ok, P_guard, P_sub) ==")
    p3 = multilevel()
    with open("results/phase3/exp3b_p3.csv", "w", newline="") as f:
        w = csv.writer(f)
        w.writerow(["L", "sigma", "P_ok", "P_guard", "P_substituicao"])
        w.writerows(p3)
    for row in p3:
        print("L=%d sigma=%.2f ok=%.5f guard=%.5f sub=%.5f" % row)
