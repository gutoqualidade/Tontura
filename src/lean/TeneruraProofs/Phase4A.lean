/-
TENERURA — Fase 4A: auditoria matemática da ALU ternária (Lean 4 core, sem Mathlib)

Modelo: dígitos `Int` em {−1,0,+1}; palavras little-endian (cabeça = LSB),
espelhando BT.hs (little-endian) e alu.rs (Vec<i8> little-endian).

Conteúdo (§8.2 do brief da Fase 4A):
  1. `Mn` (faixa) por recorrência; `Mn_pow3`: 3ⁿ = 2·Mₙ + 1.
  2. `bvalI` (Σ dᵢ·3ⁱ); `bvalI_append` (A8 generalizado); extensão por zeros
     no lado ALTO preserva valor (corolário `bvalI_zero_extend` — §8.2.1).
  3. `bvalI_range`: toda palavra de dígitos válidos satisfaz |V| ≤ Mₙ (§8.2.6).
  4. Separação V = a₀ + 3·Q (§8.2.2) e queda do LSB = arredondamento ao
     inteiro mais próximo de V/3, sem empates (§8.2.3 — forma de separação).
  5. Negação: valor e involução (§8.2.4).
  6. `addC`: especificação da cadeia de carry (valor, dígitos, carry) (§8.2.5)
     e tricotomia de overflow: carry final = 0 ⟺ |soma exata| ≤ Mₙ;
     soma > Mₙ ⟺ carry = 1; soma < −Mₙ ⟺ carry = −1 (§8.2.7).
  7. `shl`: overflow (|3V| > Mₙ) ⟺ trit de topo ≠ 0 (ambas as direções);
     `shr` preserva a janela (§8.2.7).
  8. `bvalI_inj`: injetividade da codificação de dígitos em largura fixa —
     fundamento da comparação lexicográfica (§6.5).
  9. `bval_map_val`: conexão com o modelo `Trd` de Basic.lean.
 10. `window_unique`: unicidade do representante na janela (§7).

Limitação registrada: a correção do LAÇO de `from_i64`/`from_i128` (geração do
wrap balanceado) não é formalizada aqui; é verificada por teste
(`property_from_i64_window_congruence_uniqueness`) e pelo cross-check Haskell.

NOTA DA FASE 5 (2026-11-16): este arquivo NÃO compilava como entregue na
Fase 4A — 34 erros. O `lake build` de então construía só o executável (que
importa apenas Basic.lean), de modo que a mensagem "Build completed
successfully" nunca cobriu este módulo (docs/09-relatorio-fase5.md, achado
L1). Correções desta fase: as PROVAS foram refeitas até o verificador
aceitar; os ENUNCIADOS foram mantidos, com uma exceção — `addC_spec`
quantificava `∃ w c' : Int` (a palavra `w` como inteiro, mal tipado); agora
`∃ (w : List Int) (c' : Int)`. Original em results/phase5/lean-original/.
-/

import TeneruraProofs.Basic

namespace Tenerura.Phase4A

/- ## 1. Faixa balanceada por recorrência -/

/-- Mₙ = (3ⁿ−1)/2 por recorrência (evita divisão; Mₖ₊₁ = 3·Mₖ + 1). -/
def Mn : Nat → Int
  | 0 => 0
  | k + 1 => 3 * Mn k + 1

/-- 3ⁿ = 2·Mₙ + 1 (conexão faixa ↔ potência; ímparidade de 3ⁿ). -/
theorem Mn_pow3 (k : Nat) : (3 : Int) ^ k = 2 * Mn k + 1 := by
  induction k with
  | zero => rfl
  | succ k ih =>
    have hp : (3 : Int) ^ (k + 1) = (3 : Int) ^ k * 3 := Int.pow_succ 3 k
    have hm : Mn (k + 1) = 3 * Mn k + 1 := rfl
    rw [hp, hm, ih]
    omega

/-- 3ⁿ é positivo. -/
theorem pow3I_pos (k : Nat) : 0 < (3 : Int) ^ k := by
  induction k with
  | zero =>
    rw [Int.pow_zero]
    omega
  | succ k ih =>
    rw [Int.pow_succ]
    omega

/- ## 2. Valor de palavras little-endian -/

/-- Valor de uma palavra little-endiana de dígitos `Int`: Σ dᵢ·3ⁱ. -/
def bvalI : List Int → Int
  | [] => 0
  | d :: ds => d + 3 * bvalI ds

theorem bvalI_cons (d : Int) (ds : List Int) : bvalI (d :: ds) = d + 3 * bvalI ds := rfl

/-- A8 generalizado: concatenar `ys` no lado alto adiciona 3^|xs| · bvalI ys. -/
theorem bvalI_append (xs ys : List Int) :
    bvalI (xs ++ ys) = bvalI xs + (3 : Int) ^ xs.length * bvalI ys := by
  induction xs with
  | nil =>
    show bvalI ys = 0 + (3 : Int) ^ 0 * bvalI ys
    rw [Int.pow_zero, Int.one_mul, Int.zero_add]
  | cons x xs ih =>
    have hmul : (3 : Int) ^ (xs.length + 1) * bvalI ys
              = 3 * ((3 : Int) ^ xs.length * bvalI ys) := by
      rw [Int.pow_succ, Int.mul_comm ((3 : Int) ^ xs.length) 3, Int.mul_assoc]
    show x + 3 * bvalI (xs ++ ys)
       = (x + 3 * bvalI xs) + (3 : Int) ^ (xs.length + 1) * bvalI ys
    rw [ih, hmul]
    omega

/-- §8.2.1 Extensão por zeros no lado ALTO preserva o valor
    (`width_extend` do Rust / preenchimento Z de `btAdd` do Haskell). -/
theorem bvalI_zero_extend (ds : List Int) (m : Nat) :
    bvalI (ds ++ List.replicate m 0) = bvalI ds := by
  have hz : bvalI (List.replicate m 0) = 0 := by
    induction m with
    | zero => rfl
    | succ m ih =>
      show (0 : Int) + 3 * bvalI (List.replicate m 0) = 0
      rw [ih]
      rfl
  rw [bvalI_append, hz, Int.mul_zero, Int.add_zero]

/- ## 3. Faixa representável (§8.2.6) -/

/-- Toda palavra com dígitos em {−1,0,+1} satisfaz −Mₙ ≤ V ≤ Mₙ. -/
theorem bvalI_range (ds : List Int) (h : ∀ d ∈ ds, -1 ≤ d ∧ d ≤ 1) :
    -Mn ds.length ≤ bvalI ds ∧ bvalI ds ≤ Mn ds.length := by
  induction ds with
  | nil =>
    show -(0 : Int) ≤ 0 ∧ (0 : Int) ≤ 0
    omega
  | cons d ds ih =>
    have hd : -1 ≤ d ∧ d ≤ 1 := h d (by simp)
    have hrest : ∀ e ∈ ds, -1 ≤ e ∧ e ≤ 1 := by
      intro e he
      exact h e (List.mem_cons_of_mem _ he)
    obtain ⟨lo, hi⟩ := ih hrest
    show -(3 * Mn ds.length + 1) ≤ d + 3 * bvalI ds
       ∧ d + 3 * bvalI ds ≤ 3 * Mn ds.length + 1
    omega

/- ## 4. Separação e queda do LSB (§8.2.2, §8.2.3) -/

/-- §8.2.2 Separação: V = a₀ + 3·Q com Q = valor da cauda (definicional). -/
theorem bvalI_separation (a : Int) (ds : List Int) :
    bvalI (a :: ds) = a + 3 * bvalI ds := rfl

/-- §8.2.3 Queda do LSB = arredondamento ao inteiro mais próximo, SEM EMPATES:
    se V = a₀ + 3·Q com a₀ ∈ {−1,0,+1}, então |V − 3·Q| ≤ 1 e todo inteiro
    R ≠ Q fica a distância ≥ 2 — separação estrita. -/
theorem lsb_drop_nearest (V a0 Q R : Int) (h : V = a0 + 3 * Q)
    (ha : -1 ≤ a0 ∧ a0 ≤ 1) (hR : R ≠ Q) :
    (-1 ≤ V - 3 * Q ∧ V - 3 * Q ≤ 1) ∧ (V - 3 * R ≥ 2 ∨ V - 3 * R ≤ -2) := by
  refine ⟨?_, ?_⟩ <;> omega

/-- Corolário: o quociente pela queda do LSB é ÚNICO. -/
theorem lsb_drop_unique (V a0 Q Q' : Int) (h : V = a0 + 3 * Q) (h' : V = a0 + 3 * Q') :
    Q = Q' := by
  omega

/- ## 5. Negação (§8.2.4) -/

/-- Negação digitwise sobre List Int. -/
def negI (ds : List Int) : List Int := ds.map (fun d => -d)

/-- A negação preserva o valor com sinal trocado. -/
theorem bvalI_neg (ds : List Int) : bvalI (negI ds) = -bvalI ds := by
  induction ds with
  | nil => rfl
  | cons d ds ih =>
    show -d + 3 * bvalI (negI ds) = -(d + 3 * bvalI ds)
    rw [ih]
    omega

/-- A negação é involução sobre palavras. -/
theorem negI_involutive (ds : List Int) : negI (negI ds) = ds := by
  induction ds with
  | nil => rfl
  | cons d ds ih =>
    show -(-d) :: negI (negI ds) = d :: ds
    rw [ih, Int.neg_neg]

/- ## 6. Soma com cadeia de carry (§8.2.5) -/

/-- Dígito balanceado da coluna de soma `s` (≥ −3, ≤ 3): schoolbook. -/
def balDigD (s : Int) : Int := if s ≥ 2 then s - 3 else if s ≤ -2 then s + 3 else s

/-- Carry da coluna de soma `s`. -/
def balDigC (s : Int) : Int := if s ≥ 2 then 1 else if s ≤ -2 then -1 else 0

/-- Especificação da coluna: s = d + 3·c, dígitos e carry na faixa. -/
theorem balDig_spec (s : Int) (h : -3 ≤ s ∧ s ≤ 3) :
    s = balDigD s + 3 * balDigC s
    ∧ -1 ≤ balDigD s ∧ balDigD s ≤ 1
    ∧ -1 ≤ balDigC s ∧ balDigC s ≤ 1 := by
  obtain ⟨h1, h2⟩ := h
  by_cases hp : s ≥ 2
  · simp only [balDigD, balDigC, if_pos hp]
    omega
  by_cases hn : s ≤ -2
  · simp only [balDigD, balDigC, if_neg hp, if_pos hn]
    omega
  · simp only [balDigD, balDigC, if_neg hp, if_neg hn]
    omega

/-- Cadeia de carry: dado (a, b, c) com dígitos válidos e c ∈ {−1,0,1},
    produz (w, c′) com invariante de valor, dígitos de w válidos e
    c′ ∈ {−1,0,1}. -/
def addC (a b : List Int) (c : Int) : List Int × Int :=
  match a, b with
  | [], [] => ([], c)
  | x :: xs, y :: ys =>
    (balDigD (x + y + c) :: (addC xs ys (balDigC (x + y + c))).1,
     (addC xs ys (balDigC (x + y + c))).2)
  | _, _ => ([], 0)

theorem addC_spec (a b : List Int) (c : Int) (hl : a.length = b.length)
    (hva : ∀ d ∈ a, -1 ≤ d ∧ d ≤ 1) (hvb : ∀ d ∈ b, -1 ≤ d ∧ d ≤ 1)
    (hc : -1 ≤ c ∧ c ≤ 1) :
    ∃ (w : List Int) (c' : Int),
      addC a b c = (w, c')
      ∧ (bvalI a + bvalI b + c = bvalI w + (3 : Int) ^ a.length * c')
      ∧ (∀ d ∈ w, -1 ≤ d ∧ d ≤ 1)
      ∧ (-1 ≤ c' ∧ c' ≤ 1)
      ∧ w.length = a.length := by
  induction a generalizing b c with
  | nil =>
    cases b with
    | nil =>
      refine ⟨[], c, rfl, ?_, ?_, hc, rfl⟩
      · show (0 : Int) + 0 + c = 0 + (3 : Int) ^ 0 * c
        rw [Int.pow_zero]
        omega
      · intro e he
        cases he
    | cons y ys => simp at hl
  | cons x xs ih =>
    cases b with
    | nil => simp at hl
    | cons y ys =>
      have hlen' : xs.length = ys.length := by simpa using hl
      have hx := hva x (by simp)
      have hy := hvb y (by simp)
      have hs : -3 ≤ x + y + c ∧ x + y + c ≤ 3 := by omega
      obtain ⟨hvs, hdb, hdb2, hcb, hcb2⟩ := balDig_spec (x + y + c) hs
      obtain ⟨w', c', hrec, hinv, hval', hc', hlen⟩ :=
        ih ys (balDigC (x + y + c)) hlen'
          (fun d hd => hva d (List.mem_cons_of_mem _ hd))
          (fun d hd => hvb d (List.mem_cons_of_mem _ hd))
          ⟨hcb, hcb2⟩
      refine ⟨balDigD (x + y + c) :: w', c', ?_, ?_, ?_, hc', ?_⟩
      · show (balDigD (x + y + c) :: (addC xs ys (balDigC (x + y + c))).1,
              (addC xs ys (balDigC (x + y + c))).2) = _
        rw [hrec]
      · have hmul : (3 : Int) ^ (xs.length + 1) * c'
                   = 3 * ((3 : Int) ^ xs.length * c') := by
          rw [Int.pow_succ, Int.mul_comm ((3 : Int) ^ xs.length) 3, Int.mul_assoc]
        show (x + 3 * bvalI xs) + (y + 3 * bvalI ys) + c
           = (balDigD (x + y + c) + 3 * bvalI w') + (3 : Int) ^ (xs.length + 1) * c'
        rw [hmul]
        omega
      · intro e he
        rcases List.mem_cons.mp he with rfl | hm
        · exact ⟨hdb, hdb2⟩
        · exact hval' e hm
      · show w'.length + 1 = xs.length + 1
        rw [hlen]

/-- Equivalência da spec de addC com a forma acessada por projeção. -/
theorem addC_pair (a b : List Int) (c : Int) (hl : a.length = b.length)
    (hva : ∀ d ∈ a, -1 ≤ d ∧ d ≤ 1) (hvb : ∀ d ∈ b, -1 ≤ d ∧ d ≤ 1)
    (hc : -1 ≤ c ∧ c ≤ 1) :
    addC a b c = ((addC a b c).1, (addC a b c).2) := rfl

/- ## 7. Overflow da soma (§8.2.7): tricotomia -/

/-- carry final = 0 ⟺ a soma exata está na janela (sem overflow). -/
theorem add_overflow_iff_zero (a b : List Int) (hl : a.length = b.length)
    (hva : ∀ d ∈ a, -1 ≤ d ∧ d ≤ 1) (hvb : ∀ d ∈ b, -1 ≤ d ∧ d ≤ 1) :
    (addC a b 0).2 = 0 ↔
      (-Mn a.length ≤ bvalI a + bvalI b ∧ bvalI a + bvalI b ≤ Mn a.length) := by
  obtain ⟨w, c', hrec, hinv, hval, hcb, hlen⟩ :=
    addC_spec a b 0 hl hva hvb ⟨by omega, by omega⟩
  have hrw := bvalI_range w hval
  rw [hlen] at hrw
  have hp := Mn_pow3 a.length
  rw [hrec]
  show c' = 0 ↔ _
  rcases (by omega : c' = 0 ∨ c' = 1 ∨ c' = -1) with hc | hc | hc
  · subst hc
    rw [Int.mul_zero] at hinv
    omega
  · subst hc
    rw [Int.mul_one] at hinv
    omega
  · subst hc
    rw [Int.mul_neg, Int.mul_one] at hinv
    omega

/-- soma exata > Mₙ ⟺ carry final = 1 (overflow positivo). -/
theorem add_pos_overflow (a b : List Int) (hl : a.length = b.length)
    (hva : ∀ d ∈ a, -1 ≤ d ∧ d ≤ 1) (hvb : ∀ d ∈ b, -1 ≤ d ∧ d ≤ 1) :
    (bvalI a + bvalI b > Mn a.length) ↔ (addC a b 0).2 = 1 := by
  obtain ⟨w, c', hrec, hinv, hval, hcb, hlen⟩ :=
    addC_spec a b 0 hl hva hvb ⟨by omega, by omega⟩
  have hrw := bvalI_range w hval
  rw [hlen] at hrw
  have hp := Mn_pow3 a.length
  rw [hrec]
  show _ ↔ c' = 1
  rcases (by omega : c' = 0 ∨ c' = 1 ∨ c' = -1) with hc | hc | hc
  · subst hc
    rw [Int.mul_zero] at hinv
    omega
  · subst hc
    rw [Int.mul_one] at hinv
    omega
  · subst hc
    rw [Int.mul_neg, Int.mul_one] at hinv
    omega

/-- soma exata < −Mₙ ⟺ carry final = −1 (overflow negativo). -/
theorem add_neg_overflow (a b : List Int) (hl : a.length = b.length)
    (hva : ∀ d ∈ a, -1 ≤ d ∧ d ≤ 1) (hvb : ∀ d ∈ b, -1 ≤ d ∧ d ≤ 1) :
    (bvalI a + bvalI b < -(Mn a.length)) ↔ (addC a b 0).2 = -1 := by
  obtain ⟨w, c', hrec, hinv, hval, hcb, hlen⟩ :=
    addC_spec a b 0 hl hva hvb ⟨by omega, by omega⟩
  have hrw := bvalI_range w hval
  rw [hlen] at hrw
  have hp := Mn_pow3 a.length
  rw [hrec]
  show _ ↔ c' = -1
  rcases (by omega : c' = 0 ∨ c' = 1 ∨ c' = -1) with hc | hc | hc
  · subst hc
    rw [Int.mul_zero] at hinv
    omega
  · subst hc
    rw [Int.mul_one] at hinv
    omega
  · subst hc
    rw [Int.mul_neg, Int.mul_one] at hinv
    omega

/- ## 8. Subtração = soma com a negação (§6.2) -/

/-- sub(a,b) = addC(a, negI b, 0); o valor de negI b é −bvalI b, logo o
    overflow da subtração é o da soma a + (−b) — mesmo contrato. -/
theorem sub_value (a b : List Int) (hv : ∀ d ∈ b, -1 ≤ d ∧ d ≤ 1) :
    bvalI (negI b) = -bvalI b := bvalI_neg b

/- ## 9. shl e shr (§8.2.7) -/

/-- shl: overflow (|3·V| > Mₙ) ⟺ trit de topo ≠ 0 — AMBAS as direções.
    Palavra: init ++ [top] (little-endian: topo = último dígito). -/
theorem shl_overflow_iff (init : List Int) (top : Int)
    (hv : ∀ d ∈ init ++ [top], -1 ≤ d ∧ d ≤ 1) :
    (top ≠ 0 ↔
      (3 * bvalI (init ++ [top]) > Mn (init ++ [top]).length
       ∨ 3 * bvalI (init ++ [top]) < -(Mn (init ++ [top]).length))) := by
  have happ : bvalI (init ++ [top])
            = bvalI init + (3 : Int) ^ init.length * top := by
    rw [bvalI_append]
    show bvalI init + (3 : Int) ^ init.length * (top + 3 * 0) = _
    rw [Int.mul_zero, Int.add_zero]
  have hr := bvalI_range init (fun d hd => hv d (List.mem_append_left _ hd))
  have htop : -1 ≤ top ∧ top ≤ 1 := hv top (by simp)
  have hlen : (init ++ [top]).length = init.length + 1 := by simp
  have hP := Mn_pow3 init.length
  have hMn1 : Mn (init.length + 1) = 3 * Mn init.length + 1 := rfl
  rw [hlen, hMn1, happ]
  rcases (by omega : top = 0 ∨ top = 1 ∨ top = -1) with ht | ht | ht
  · subst ht
    rw [Int.mul_zero, Int.add_zero]
    omega
  · subst ht
    rw [Int.mul_one]
    omega
  · subst ht
    rw [Int.mul_neg, Int.mul_one]
    omega

/-- shr: o valor entregue (cauda da palavra) está na janela de n−1 trits. -/
theorem shr_window (d : Int) (rest : List Int)
    (hv : ∀ e ∈ d :: rest, -1 ≤ e ∧ e ≤ 1) :
    -Mn rest.length ≤ bvalI rest ∧ bvalI rest ≤ Mn rest.length :=
  bvalI_range rest (fun e he => hv e (List.mem_cons_of_mem _ he))

/- ## 10. Injetividade da codificação (fundamento de cmp, §6.5) -/

/-- Em largura fixa, a codificação digitwise é injetiva: dois códigos com o
    mesmo valor são a MESMA palavra. Prova: no par de cabeças, a₀ − b₀ =
    3·(B − A) ∈ [−2,2] força a₀ = b₀; indução na cauda. -/
theorem bvalI_inj : ∀ (n : Nat) (a b : List Int), a.length = n → b.length = n →
    (∀ d ∈ a, -1 ≤ d ∧ d ≤ 1) → (∀ d ∈ b, -1 ≤ d ∧ d ≤ 1) →
    bvalI a = bvalI b → a = b := by
  intro n
  induction n with
  | zero =>
    intro a b ha hb _ _ _
    cases a with
    | nil =>
      cases b with
      | nil => rfl
      | cons y ys => simp at hb
    | cons x xs => simp at ha
  | succ n ih =>
    intro a b ha hb hva hvb hval
    cases a with
    | nil => simp at ha
    | cons x xs =>
      cases b with
      | nil => simp at hb
      | cons y ys =>
        have hx := hva x (by simp)
        have hy := hvb y (by simp)
        have hval' : x + 3 * bvalI xs = y + 3 * bvalI ys := hval
        have hxy : x = y := by omega
        have hxs : bvalI xs = bvalI ys := by omega
        have hla : xs.length = n := by simpa using ha
        have hlb : ys.length = n := by simpa using hb
        have hva' : ∀ d ∈ xs, -1 ≤ d ∧ d ≤ 1 := by
          intro d hd; exact hva d (List.mem_cons_of_mem _ hd)
        have hvb' : ∀ d ∈ ys, -1 ≤ d ∧ d ≤ 1 := by
          intro d hd; exact hvb d (List.mem_cons_of_mem _ hd)
        rw [hxy, ih xs ys hla hlb hva' hvb' hxs]

/- ## 11. Conexão com o modelo Trd de Basic.lean -/

/-- O modelo Int coincide com bval (List Trd) de Basic.lean sob Trd.val. -/
theorem bval_map_val (ds : List Trd) : bvalI (ds.map Trd.val) = bval ds := by
  induction ds with
  | nil => rfl
  | cons d ds ih =>
    show Trd.val d + 3 * bvalI (ds.map Trd.val) = Trd.val d + 3 * bval ds
    rw [ih]

/- ## 12. Unicidade do representante na janela (§7) -/

/-- Na janela balanceada [−Mₙ, Mₙ] há UMA única representante por classe
    módulo 3ⁿ (a janela tem largura exatamente 3ⁿ). Fundamenta a unicidade
    do wrap entregue por from_i64/mul (o laço em si fica como limitação
    verificada por teste). -/
theorem window_unique (n : Nat) (q q' k : Int)
    (h1 : -Mn n ≤ q ∧ q ≤ Mn n) (h2 : -Mn n ≤ q' ∧ q' ≤ Mn n)
    (h3 : q - q' = (3 : Int) ^ n * k) : q = q' := by
  have hp := Mn_pow3 n
  have hk : k = 0 ∨ k ≥ 1 ∨ k ≤ -1 := by omega
  rcases hk with hk | hk | hk
  · subst hk
    omega
  · have hge : (3 : Int) ^ n ≤ (3 : Int) ^ n * k := by
      have h0 := Int.mul_le_mul_of_nonneg_left (a := (1 : Int)) (b := k)
        (c := (3 : Int) ^ n) (by omega) (by have := pow3I_pos n; omega)
      rw [Int.mul_one] at h0
      exact h0
    omega
  · have hle : (3 : Int) ^ n * k ≤ -((3 : Int) ^ n) := by
      have h0 := Int.mul_le_mul_of_nonneg_left (a := k) (b := (-1 : Int))
        (c := (3 : Int) ^ n) (by omega) (by have := pow3I_pos n; omega)
      rw [Int.mul_neg, Int.mul_one] at h0
      exact h0
    omega

end Tenerura.Phase4A
