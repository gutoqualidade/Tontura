/-
TENERURA — Fase 3A: simetria dos números ímpares (Lean 4 core, sem Mathlib)

Teoremas alvo (§2 do plano da Fase 3):
  T1. 3^n é ímpar para todo n (fonte da faixa balanceada simétrica).
  T2. Em Z_N com N ímpar, o único ponto fixo da negação é 0.
  T3. Em Z_N com N par (N ≥ 2), existe segundo ponto fixo: N/2.
  T4. A negação (em ℤ) é involução.
  T5. A faixa balanceada [−M, M] com M = (3ⁿ−1)/2 é simétrica para TODO n —
      a simetria não exige n ímpar (independência paridade × faixa).

NOTA DA FASE 5 (2026-11-16): este arquivo NÃO compilava como entregue na
Fase 3 (o `lake build` de então só construía o executável, que importa
apenas Basic.lean — ver docs/09-relatorio-fase5.md §"Achado L1"). Os
ENUNCIADOS foram mantidos (trocando só a notação `ℤ`, que não existe sem
Mathlib, por `Int`); as PROVAS foram corrigidas até o verificador aceitar.
Original preservado em results/phase5/lean-original/Phase3.lean.
-/

namespace Tenerura.Phase3

/-- T1. 3^n é ímpar para todo n (indução + Nat.mul_mod). -/
theorem pow3_odd (n : Nat) : 3 ^ n % 2 = 1 := by
  induction n with
  | zero => rfl
  | succ n ih => rw [Nat.pow_succ, Nat.mul_mod, ih]

/-- Lema de paridade: se N é ímpar, N·(2m+1) não pode ser par.
    (expansão distributiva manual; o `omega` fecha com os produtos como átomos) -/
theorem odd_mul_odd_not_even (N m x : Nat) (hN : N % 2 = 1)
    (h : 2 * x = N * (2 * m + 1)) : False := by
  have hd : N * (2 * m + 1) = 2 * (N * m) + N := by
    rw [Nat.left_distrib, Nat.mul_one, Nat.mul_left_comm]
  omega

/-- T2. Unicidade do ponto fixo da negação módulo N ímpar:
    2x ≡ 0 (mod N) ∧ N ímpar → x ≡ 0 (mod N). -/
theorem fixed_point_odd_unique (N x : Nat) (hN : N % 2 = 1)
    (h : 2 * x % N = 0) : x % N = 0 := by
  have hdm : N * (2 * x / N) + 2 * x % N = 2 * x := Nat.div_add_mod (2 * x) N
  rw [h, Nat.add_zero] at hdm
  -- hdm : N * (2*x/N) = 2*x  (o quociente k := 2*x/N é o candidato)
  generalize 2 * x / N = k at hdm
  rcases Nat.mod_two_eq_zero_or_one k with he | ho
  · -- k par: k = 2m ⇒ 2x = 2·(N·m) ⇒ x = N·m ⇒ N ∣ x
    obtain ⟨m, hm⟩ : ∃ m, k = 2 * m := ⟨k / 2, by omega⟩
    subst hm
    have hc : N * (2 * m) = 2 * (N * m) := Nat.mul_left_comm N 2 m
    have hx : x = N * m := by omega
    rw [hx]
    exact Nat.mul_mod_right N m
  · -- k ímpar: 2x = N·(2m+1) contradiz N ímpar
    obtain ⟨m, hm⟩ : ∃ m, k = 2 * m + 1 := ⟨k / 2, by omega⟩
    subst hm
    exact (odd_mul_odd_not_even N m x hN hdm.symm).elim

/-- T3. Segundo ponto fixo para N par: x = N/2 é auto-oposto e distinto de 0. -/
theorem fixed_point_even_second (N : Nat) (hN : N % 2 = 0) (hN2 : 2 ≤ N) :
    2 * (N / 2) % N = 0 ∧ (N / 2) % N = N / 2 ∧ (N / 2) % N ≠ 0 := by
  have h2 : 2 * (N / 2) = N := by omega
  refine ⟨?_, ?_, ?_⟩
  · rw [h2]
    exact Nat.mod_self N
  · exact Nat.mod_eq_of_lt (by omega)
  · rw [Nat.mod_eq_of_lt (by omega)]
    omega

/-- T4. A negação em ℤ é involução. -/
theorem neg_involutive (x : Int) : -(-x) = x := Int.neg_neg x

/-- Conexão faixa ↔ 3ⁿ: M = (3ⁿ−1)/2 satisfaz 3ⁿ = 2M+1 (para todo n). -/
theorem balanced_half (n : Nat) : (3 ^ n : Nat) = 2 * ((3 ^ n - 1) / 2) + 1 := by
  have h := pow3_odd n
  generalize 3 ^ n = p at h ⊢
  omega

/-- T5. Simetria da faixa balanceada para TODO n (sem hipótese de paridade). -/
theorem range_symmetry_3n (n : Nat) (M x : Int)
    (hM : M = (((3 ^ n - 1) / 2 : Nat) : Int))
    (h : -M ≤ x ∧ x ≤ M) : -M ≤ -x ∧ -x ≤ M := by
  omega

/-- T5'. Instância concreta com n par de trits (n=2, 9 estados): simétrica. -/
example (x : Int) (h : -4 ≤ x ∧ x ≤ 4) : -4 ≤ -x ∧ -x ≤ 4 := by omega

end Tenerura.Phase3
