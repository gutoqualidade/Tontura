/-
TENERURA — Fase 5: propriedades das semânticas NOVAS introduzidas pela ISA T27
(Lean 4 core, sem Mathlib). Especificação: docs/07-isa-t27.md.

Conteúdo:
  1. Divisão centrada (instruções DIV/REM): especificação `CDiv`, unicidade
     sem empate (`cdiv_unique`), existência por um algoritmo de referência
     (`cdiv_exists`), regra de empate "para o mais próximo de zero"
     (`cdiv_tie_toward_zero`), simetria de negação (`cdiv_neg`).
  2. Coerência com o deslocamento: dividir por 3ᵏ É descartar k trits baixos
     (`cdiv_pow3_is_trunc`, `div_pow3_drops_low_digits`) — não existe em T27
     a divergência "sar ≠ idiv" do binário.
  3. Decomposição página/deslocamento: existe e é única
     (`page_split_exists`, `page_split_unique`) — páginas centradas
     particionam a reta de endereços; a mesma prova cobre "PTE = centro do
     quadro + flags" e os dois níveis da tabela de páginas.
  4. Constantes sem ajuste de empréstimo (LUI + ADDI): `Mn_add`,
     `split_hi_range`.
  5. Relógio modular: a diferença calculada na janela é a diferença real
     (`serial_difference_exact`, corolário de `window_unique` da Fase 4A).

Limitação registrada: `cdivq` abaixo é um algoritmo de REFERÊNCIA sobre a
divisão euclidiana de `Int`. A implementação do emulador (`Word::divrem`, por
divisão truncada) e a da referência Haskell (por comparação de erros) NÃO são
formalizadas aqui; a concordância entre as três é verificada por teste
(tests/word_diff.rs e tools/isa-crosscheck.sh), não por prova.
-/

import TeneruraProofs.Phase4A

namespace Tenerura.Phase5

open Tenerura.Phase4A

/- ## 0. Auxiliares: produto por um fator positivo -/

theorem mul_ge_of_ge_one (d b : Int) (hd : 1 ≤ d) (hb : 0 < b) : b ≤ d * b := by
  have h := Int.mul_le_mul_of_nonneg_right (a := 1) (b := d) (c := b) hd (by omega)
  rw [Int.one_mul] at h
  exact h

theorem mul_ge_of_ge_two (d b : Int) (hd : 2 ≤ d) (hb : 0 < b) : 2 * b ≤ d * b :=
  Int.mul_le_mul_of_nonneg_right (a := 2) (b := d) (c := b) hd (by omega)

theorem mul_le_of_le_neg_one (d b : Int) (hd : d ≤ -1) (hb : 0 < b) : d * b ≤ -b := by
  have h := Int.mul_le_mul_of_nonneg_right (a := d) (b := -1) (c := b) hd (by omega)
  rw [Int.neg_one_mul] at h
  exact h

/- ## 1. Divisão centrada -/

/-- Especificação (divisor positivo): a = q·b + r com −b ≤ 2r ≤ b,
    isto é, q é um inteiro mais próximo de a/b. -/
def CDiv (a b q r : Int) : Prop := a = q * b + r ∧ -b ≤ 2 * r ∧ 2 * r ≤ b

/-- Unicidade sem empate: se um par satisfaz a especificação com resto
    ESTRITAMENTE dentro de meio divisor, todo outro par válido coincide. -/
theorem cdiv_unique (a b q r q' r' : Int) (hb : 0 < b)
    (h : CDiv a b q r) (hs : -b < 2 * r ∧ 2 * r < b) (h' : CDiv a b q' r') :
    q = q' ∧ r = r' := by
  obtain ⟨he, _, _⟩ := h
  obtain ⟨he', hl', hu'⟩ := h'
  have hd : (q - q') * b = r' - r := by
    rw [Int.sub_mul]
    omega
  have hcase : q - q' = 0 ∨ 1 ≤ q - q' ∨ q - q' ≤ -1 := by omega
  rcases hcase with h0 | h1 | h2
  · have hq : q = q' := by omega
    subst hq
    exact ⟨rfl, by omega⟩
  · have := mul_ge_of_ge_one (q - q') b h1 hb
    omega
  · have := mul_le_of_le_neg_one (q - q') b h2 hb
    omega

/-- Algoritmo de referência (divisor positivo): quociente mais próximo; no
    empate (2·resto = b), o mais próximo de zero. -/
def cdivq (a b : Int) : Int :=
  if 2 * (a % b) < b then a / b
  else if b < 2 * (a % b) then a / b + 1
  else if 0 ≤ a / b then a / b else a / b + 1

def cdivr (a b : Int) : Int := a - cdivq a b * b

/-- Existência: o algoritmo satisfaz a especificação para todo divisor positivo. -/
theorem cdiv_exists (a b : Int) (hb : 0 < b) : CDiv a b (cdivq a b) (cdivr a b) := by
  have hm := Int.emod_add_ediv a b
  have hnn := Int.emod_nonneg a (b := b) (by omega)
  have hlt := Int.emod_lt_of_pos a hb
  have hc : b * (a / b) = (a / b) * b := Int.mul_comm _ _
  have h1 : (a / b + 1) * b = (a / b) * b + b := by rw [Int.add_mul, Int.one_mul]
  unfold CDiv cdivr cdivq
  by_cases c1 : 2 * (a % b) < b
  · simp only [c1, if_true]
    omega
  · by_cases c2 : b < 2 * (a % b)
    · simp only [c1, c2, if_true, if_false]
      omega
    · by_cases c3 : 0 ≤ a / b
      · simp only [c1, c2, c3, if_true, if_false]
        omega
      · simp only [c1, c2, c3, if_true, if_false]
        omega

/-- Todo quociente válido é o piso ou o piso mais um. -/
theorem cdiv_candidates (a b q' r' : Int) (hb : 0 < b) (h' : CDiv a b q' r') :
    (q' = a / b ∧ r' = a % b) ∨ (q' = a / b + 1 ∧ r' = a % b - b) := by
  obtain ⟨he', hl', hu'⟩ := h'
  have hm := Int.emod_add_ediv a b
  have hnn := Int.emod_nonneg a (b := b) (by omega)
  have hlt := Int.emod_lt_of_pos a hb
  have hc : b * (a / b) = (a / b) * b := Int.mul_comm _ _
  have hd : (q' - a / b) * b = a % b - r' := by
    rw [Int.sub_mul]
    omega
  have hcase : q' - a / b ≤ -1 ∨ q' - a / b = 0 ∨ q' - a / b = 1 ∨ 2 ≤ q' - a / b := by omega
  rcases hcase with h | h | h | h
  · have := mul_le_of_le_neg_one _ b h hb
    omega
  · rw [h, Int.zero_mul] at hd
    left
    omega
  · rw [h, Int.one_mul] at hd
    right
    omega
  · have := mul_ge_of_ge_two _ b h hb
    omega

/-- Regra de empate: o quociente do algoritmo nunca tem módulo maior que o
    de outro quociente válido (|q| ≤ |q'|, escrito sem valor absoluto). -/
theorem cdiv_tie_toward_zero (a b q' r' : Int) (hb : 0 < b) (h' : CDiv a b q' r') :
    (cdivq a b ≤ q' ∧ -q' ≤ cdivq a b) ∨ (cdivq a b ≤ -q' ∧ q' ≤ cdivq a b) := by
  have hcand := cdiv_candidates a b q' r' hb h'
  obtain ⟨_, hl', hu'⟩ := h'
  have hnn := Int.emod_nonneg a (b := b) (by omega)
  have hlt := Int.emod_lt_of_pos a hb
  unfold cdivq
  by_cases c1 : 2 * (a % b) < b
  · simp only [c1, if_true]
    omega
  · by_cases c2 : b < 2 * (a % b)
    · simp only [c1, c2, if_true, if_false]
      omega
    · by_cases c3 : 0 ≤ a / b
      · simp only [c1, c2, c3, if_true, if_false]
        omega
      · simp only [c1, c2, c3, if_true, if_false]
        omega

/-- Simetria de negação: negar o dividendo nega quociente e resto. -/
theorem cdiv_neg (a b q r : Int) (h : CDiv a b q r) : CDiv (-a) b (-q) (-r) := by
  obtain ⟨he, hl, hu⟩ := h
  refine ⟨?_, by omega, by omega⟩
  rw [Int.neg_mul]
  omega

/- ## 2. Dividir por 3ᵏ é descartar k trits baixos -/

/-- Se a = lo + 3ᵏ·hi com lo na janela de k trits, todo par válido da
    divisão centrada por 3ᵏ tem q = hi e r = lo (não há empate: 3ᵏ é ímpar). -/
theorem cdiv_pow3_is_trunc (k : Nat) (a lo hi q r : Int)
    (hsplit : a = lo + (3 : Int) ^ k * hi) (hlo : -Mn k ≤ lo ∧ lo ≤ Mn k)
    (h : CDiv a ((3 : Int) ^ k) q r) : q = hi ∧ r = lo := by
  have hp := Mn_pow3 k
  have hpos := pow3I_pos k
  have hc : (3 : Int) ^ k * hi = hi * (3 : Int) ^ k := Int.mul_comm _ _
  have hspec : CDiv a ((3 : Int) ^ k) hi lo := ⟨by omega, by omega, by omega⟩
  have hu := cdiv_unique a _ hi lo q r hpos hspec ⟨by omega, by omega⟩ h
  exact ⟨hu.1.symm, hu.2.symm⟩

/-- Versão sobre dígitos: o quociente centrado de uma palavra por 3^|xs| é o
    valor dos dígitos ALTOS, e o resto é o valor dos dígitos baixos — DIV por
    potência de 3 coincide com o deslocamento à direita (trunc_low). -/
theorem div_pow3_drops_low_digits (xs ys : List Int) (hx : ∀ d ∈ xs, -1 ≤ d ∧ d ≤ 1)
    (q r : Int) (h : CDiv (bvalI (xs ++ ys)) ((3 : Int) ^ xs.length) q r) :
    q = bvalI ys ∧ r = bvalI xs :=
  cdiv_pow3_is_trunc xs.length _ (bvalI xs) (bvalI ys) q r (bvalI_append xs ys)
    (bvalI_range xs hx) h

/- ## 3. Página e deslocamento -/

/-- Todo endereço tem uma decomposição (página, deslocamento) com o
    deslocamento na janela balanceada de k trits. -/
theorem page_split_exists (k : Nat) (a : Int) :
    ∃ p o : Int, a = p * (3 : Int) ^ k + o ∧ -Mn k ≤ o ∧ o ≤ Mn k := by
  have hp := Mn_pow3 k
  have hpos := pow3I_pos k
  obtain ⟨he, hl, hu⟩ := cdiv_exists a _ hpos
  exact ⟨cdivq a _, cdivr a _, he, by omega, by omega⟩

/-- A decomposição é ÚNICA: as páginas centradas particionam a reta de
    endereços (não se sobrepõem e não deixam buracos). O mesmo enunciado,
    com k = 6, diz que "PTE = centro do quadro + flags" se decompõe sem
    ambiguidade. -/
theorem page_split_unique (k : Nat) (a p o p' o' : Int)
    (h : a = p * (3 : Int) ^ k + o) (ho : -Mn k ≤ o ∧ o ≤ Mn k)
    (h' : a = p' * (3 : Int) ^ k + o') (ho' : -Mn k ≤ o' ∧ o' ≤ Mn k) :
    p = p' ∧ o = o' := by
  have hp := Mn_pow3 k
  have hpos := pow3I_pos k
  exact cdiv_unique a _ p o p' o' hpos ⟨h, by omega, by omega⟩ ⟨by omega, by omega⟩
    ⟨h', by omega, by omega⟩

/- ## 4. Constantes sem ajuste de empréstimo -/

/-- M_{m+n} = 3ⁿ·M_m + M_n: a janela de m+n trits é a de m trits "em cima"
    da de n trits. -/
theorem Mn_add (m n : Nat) : Mn (m + n) = (3 : Int) ^ n * Mn m + Mn n := by
  induction n with
  | zero =>
    show Mn m = (3 : Int) ^ 0 * Mn m + 0
    rw [Int.pow_zero, Int.one_mul, Int.add_zero]
  | succ n ih =>
    have h1 : Mn (m + (n + 1)) = 3 * Mn (m + n) + 1 := rfl
    have h2 : Mn (n + 1) = 3 * Mn n + 1 := rfl
    have h3 : (3 : Int) ^ (n + 1) * Mn m = 3 * ((3 : Int) ^ n * Mn m) := by
      rw [Int.pow_succ, Int.mul_comm ((3 : Int) ^ n) 3, Int.mul_assoc]
    rw [h1, ih, h2, h3]
    omega

/-- Se v cabe em m+n trits e v = hi·3ⁿ + lo com lo na janela de n trits,
    então hi cabe em m trits. Consequência para a ISA: qualquer constante de
    27 trits é LUI (11 trits altos) + ADDI (16 trits baixos) com os campos
    lidos DIRETO dos dígitos — sem o ajuste de empréstimo que a extensão de
    sinal impõe às ISAs binárias. -/
theorem split_hi_range (m n : Nat) (v hi lo : Int)
    (hv : -Mn (m + n) ≤ v ∧ v ≤ Mn (m + n))
    (h : v = hi * (3 : Int) ^ n + lo) (hlo : -Mn n ≤ lo ∧ lo ≤ Mn n) :
    -Mn m ≤ hi ∧ hi ≤ Mn m := by
  have hadd := Mn_add m n
  have hp := Mn_pow3 n
  have hpos := pow3I_pos n
  have hc : (3 : Int) ^ n * Mn m = Mn m * (3 : Int) ^ n := Int.mul_comm _ _
  refine ⟨?_, ?_⟩
  · by_cases hneg : hi ≤ -Mn m - 1
    · have h1 := Int.mul_le_mul_of_nonneg_right (a := hi) (b := -Mn m - 1)
        (c := (3 : Int) ^ n) hneg (by omega)
      have h2 : (-Mn m - 1) * (3 : Int) ^ n = -(Mn m * (3 : Int) ^ n) - (3 : Int) ^ n := by
        rw [Int.sub_mul, Int.neg_mul, Int.one_mul]
      omega
    · omega
  · by_cases hbig : Mn m + 1 ≤ hi
    · have h1 := Int.mul_le_mul_of_nonneg_right (a := Mn m + 1) (b := hi)
        (c := (3 : Int) ^ n) hbig (by omega)
      have h2 : (Mn m + 1) * (3 : Int) ^ n = Mn m * (3 : Int) ^ n + (3 : Int) ^ n := by
        rw [Int.add_mul, Int.one_mul]
      omega
    · omega

/- ## 5. Relógio modular -/

/-- O núcleo guarda prazos como palavras e decide "já venceu?" pelo SINAL da
    diferença calculada na janela. Se a diferença real cabe na janela (os
    prazos ficam a menos de meia volta — o teto SLEEP_MAX garante isso), a
    diferença calculada É a real. -/
theorem serial_difference_exact (n : Nat) (delta real k : Int)
    (hd : -Mn n ≤ delta ∧ delta ≤ Mn n)
    (hr : -Mn n ≤ real ∧ real ≤ Mn n)
    (hc : delta - real = (3 : Int) ^ n * k) : delta = real :=
  window_unique n delta real k hd hr hc

end Tenerura.Phase5
