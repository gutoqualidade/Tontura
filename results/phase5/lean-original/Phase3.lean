/-
TENERURA — Fase 3A: simetria dos números ímpares (Lean 4 core, sem Mathlib)

Teoremas alvo (§2 do plano da Fase 3):
  T1. 3^n é ímpar para todo n (fonte da faixa balanceada simétrica).
  T2. Em Z_N com N ímpar, o único ponto fixo da negação é 0.
  T3. Em Z_N com N par (N ≥ 2), existe segundo ponto fixo: N/2.
  T4. A negação (em ℤ) é involução.
  T5. A faixa balanceada [−M, M] com M = (3ⁿ−1)/2 é simétrica para TODO n —
      a simetria não exige n ímpar (independência paridade × faixa).
-/

namespace Tenerura.Phase3

/-- T1. 3^n é ímpar para todo n (indução + Nat.mul_mod). -/
theorem pow3_odd (n : Nat) : 3 ^ n % 2 = 1 := by
  induction n with
  | zero => rfl
  | succ n ih =>
    rw [Nat.pow_succ, Nat.mul_mod, ih]
    rfl

/-- Lema de paridade: se N é ímpar, N·(2m+1) não pode ser par.
    (expansão distributiva manual; o `omega` fecha com os produtos como átomos) -/
theorem odd_mul_odd_not_even (N m x : Nat) (hN : N % 2 = 1)
    (h : 2 * x = N * (2 * m + 1)) : False := by
  have hd : N * (2 * m + 1) = 2 * (N * m) + N := by
    rw [Nat.left_distrib, Nat.mul_one]
    simp [Nat.mul_comm, Nat.mul_left_comm, Nat.mul_assoc]
  omega

/-- T2. Unicidade do ponto fixo da negação módulo N ímpar:
    2x ≡ 0 (mod N) ∧ N ímpar → x ≡ 0 (mod N). -/
theorem fixed_point_odd_unique (N x : Nat) (hN : N % 2 = 1)
    (h : 2 * x % N = 0) : x % N = 0 := by
  have hdm : N * (2 * x / N) + 2 * x % N = 2 * x := Nat.div_add_mod (2 * x) N
  rw [h, Nat.add_zero] at hdm
  -- hdm : N * (2*x/N) = 2*x  (o quociente k := 2*x/N é o candidato)
  rcases Nat.mod_two_eq_zero_or_one (2 * x / N) with he | ho
  · -- k par: k = 2m ⇒ 2x = 2·(N·m) ⇒ x = N·m ⇒ N ∣ x
    obtain ⟨m, hm⟩ : ∃ m, 2 * x / N = 2 * m := ⟨(2 * x / N) / 2, by omega⟩
    have hxe : 2 * x = 2 * (N * m) := by
      rw [← hm, ← Nat.mul_assoc, Nat.mul_left_comm] at hdm
      exact hdm
    have hx : x = N * m := Nat.mul_left_cancel_iff (n := 2) (by decide) |>.mp hxe
    rw [hx, Nat.mul_mod_right]
  · -- k ímpar: 2x = N·(2m+1) contradiz N ímpar
    obtain ⟨m, hm⟩ : ∃ m, 2 * x / N = 2 * m + 1 := ⟨(2 * x / N) / 2, by omega⟩
    rw [hm] at hdm
    exact odd_mul_odd_not_even N m x hN hdm

/-- T3. Segundo ponto fixo para N par: x = N/2 é auto-oposto e distinto de 0. -/
theorem fixed_point_even_second (N : Nat) (hN : N % 2 = 0) (hN2 : 2 ≤ N) :
    2 * (N / 2) % N = 0 ∧ (N / 2) % N = N / 2 ∧ (N / 2) % N ≠ 0 := by
  refine ⟨?_, ?_, ?_⟩
  · rw [Nat.div_add_mod]  -- 2*(N/2) = N − N%2 = N
    have h0 : N % 2 = 0 := hN
    simp [h0, Nat.mod_self]
  · exact Nat.mod_eq_of_lt (by omega)
  · rw [Nat.mod_eq_of_lt (by omega)]
    omega

/-- T4. A negação em ℤ é involução. -/
theorem neg_involutive (x : ℤ) : -(-x) = x := Int.neg_neg x

/-- Conexão faixa ↔ 3ⁿ: M = (3ⁿ−1)/2 satisfaz 3ⁿ = 2M+1 (para todo n). -/
theorem balanced_half (n : Nat) : (3 ^ n : Nat) = 2 * ((3 ^ n - 1) / 2) + 1 := by
  have h := pow3_odd n
  omega

/-- T5. Simetria da faixa balanceada para TODO n (sem hipótese de paridade). -/
theorem range_symmetry_3n (n : Nat) (M x : ℤ)
    (hM : M = (((3 ^ n : Nat) - 1) / 2 : Nat))
    (h : -M ≤ x ∧ x ≤ M) : -M ≤ -x ∧ -x ≤ M := by
  omega

/-- T5'. Instância concreta com n par de trits (n=2, 9 estados): simétrica. -/
example (x : ℤ) (h : -4 ≤ x ∧ x ≤ 4) : -4 ≤ -x ∧ -x ≤ 4 := by omega

end Tenerura.Phase3
