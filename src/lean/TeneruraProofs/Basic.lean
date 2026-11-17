/-
TENERURA — Provas Lean 4 (core apenas, sem Mathlib nesta fase)
Alvo inicial: A8 (shift = ×3^k). Próximos: A2/A3, estrutura de A1/A6.

Modelo de ouro espelhando BT.hs:
  dígito d ∈ {-1,0,1}; valor da palavra little-endian Σ dᵢ·3ⁱ.
-/

namespace Tenerura

/-- Dígito ternário balanceado. -/
inductive Trd | m | z | p
deriving Repr, DecidableEq

def Trd.val : Trd → Int
  | .m => -1
  | .z =>  0
  | .p =>  1

/-- Valor de uma palavra little-endiana: Σ dᵢ·3ⁱ. -/
def bval : List Trd → Int
  | [] => 0
  | d :: ds => Trd.val d + 3 * bval ds

/-- A8: inserir k dígitos z na frente multiplica o valor por 3^k. -/
theorem bval_shift (ds : List Trd) : ∀ k : Nat,
    bval (List.replicate k Trd.z ++ ds) = (3 ^ k) * bval ds := by
  intro k
  induction k with
  | zero =>
    show bval ds = ((3 : Int) ^ 0) * bval ds
    have h0 : ((3 : Int) ^ 0) = 1 := rfl
    rw [h0, Int.one_mul]
  | succ k ih =>
    have hrep : List.replicate (k + 1) Trd.z ++ ds
              = Trd.z :: (List.replicate k Trd.z ++ ds) := rfl
    rw [hrep]
    simp only [bval]
    have hz : Trd.val Trd.z = 0 := rfl
    rw [hz, Int.zero_add, ih]
    have hp : ((3 : Int) ^ (k + 1)) = ((3 : Int) ^ k) * 3 := Int.pow_succ 3 k
    rw [hp]
    simp [Int.mul_comm, Int.mul_assoc]

end Tenerura
