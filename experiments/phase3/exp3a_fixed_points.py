#!/usr/bin/env python3
# TENERURA Fase 3A — pontos fixos da negação módulo N (exaustivo, N=1..64)
# Hipótese: |{x ∈ Z_N : 2x ≡ 0 mod N}| = 1 se N ímpar; = 2 se N par (N>0).
# Definições: ponto fixo de f(x) = −x em Z_N; verificação exaustiva por força bruta.
# Método: enumeração completa (sem amostragem); sem sementes (determinístico).
# Limitações: N pequeno; não é prova (a prova está em Lean: fixed_point_odd_unique /
#   fixed_point_even_second); papel: verificação cruzada independente.
# Saída: results/phase3/exp3a_fixed_points.csv (bruto) + resumo no stdout.
import csv, sys

rows = []
anomalies = []
for N in range(1, 65):
    fps = [x for x in range(N) if (2 * x) % N == 0]  # x ≡ −x ⟺ 2x ≡ 0
    expected = 1 if N % 2 == 1 else 2
    ok = len(fps) == expected
    if not ok:
        anomalies.append((N, fps))
    rows.append((N, len(fps), fps, expected, ok))

# Involution check: f(f(x)) = x para todo N, x (f(x) = (N - x) % N)
inv_ok = all(((N - ((N - x) % N)) % N) == x for N in range(1, 65) for x in range(N))

# Simetria da faixa balanceada para n=1..12: |x| ≤ (3^n−1)/2 ⟺ |−x| ≤ idem (trivial, mas verificado)
sym_ok = all(
    (abs(x) <= (3**n - 1) // 2) == (abs(-x) <= (3**n - 1) // 2)
    for n in range(1, 13) for x in range(-(3**n - 1) // 2, (3**n - 1) // 2 + 1)
)

with open(sys.argv[1] if len(sys.argv) > 1 else "results/phase3/exp3a_fixed_points.csv", "w") as f:
    w = csv.writer(f)
    w.writerow(["N", "num_fixed_points", "fixed_points", "expected", "match"])
    w.writerows(rows)

print(f"exaustivo N=1..64: {'OK' if not anomalies else 'ANOMALIAS: ' + str(anomalies)}")
print(f"involution f(f(x))=x, N=1..64: {'OK' if inv_ok else 'FALHA'}")
print(f"simetria faixa balanceada n=1..12: {'OK' if sym_ok else 'FALHA'}")
par_odd = {N: len([r for r in rows if r[0] == N][0][2]) for N in (3, 5, 9, 15, 27, 33, 63)}
par_even = {N: len([r for r in rows if r[0] == N][0][2]) for N in (4, 6, 10, 12, 27 + 1, 64)}
print(f"amostras ímpares (N: pontos fixos): {par_odd}")
print(f"amostras pares  (N: pontos fixos): {par_even}")
