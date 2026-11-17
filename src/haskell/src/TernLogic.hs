-- TENERURA — lógicas ternárias sobre o mesmo portador, tipos distintos (B4)
module TernLogic
  ( kAnd, kOr, kNot, kImpl        -- Kleene K3 (forte)
  , lAnd, lOr, lNot, lImpl        -- Łukasiewicz Ł3
  , allKleene, allLukas           -- enumerações totais para testes
  ) where

import Trit

-- ---------------------------------------------------------------
-- Kleene K3 (forte). Mapeamento: M = Falso, Z = Indeterminado, P = Verdadeiro.
-- ∧ = min da ordem F ≤ U ≤ T, ∨ = max, ¬ forte (inverte F/T, U→U).
-- Implicação material: ¬a ∨ b. (Fato B2: sem tautologias não triviais.)
-- ---------------------------------------------------------------

kNot :: KleeneTrit -> KleeneTrit
kNot (KleeneTrit M) = KleeneTrit P
kNot (KleeneTrit Z) = KleeneTrit Z
kNot (KleeneTrit P) = KleeneTrit M

kAnd :: KleeneTrit -> KleeneTrit -> KleeneTrit
kAnd a b = min a b

kOr :: KleeneTrit -> KleeneTrit -> KleeneTrit
kOr a b = max a b

kImpl :: KleeneTrit -> KleeneTrit -> KleeneTrit
kImpl a b = kOr (kNot a) b

-- ---------------------------------------------------------------
-- Łukasiewicz Ł3. Valores 0, 1/2, 1 representados como M, Z, P (int 0,1,2).
-- Implicação: min(1, 1−a+b) → em inteiros: min(2, 2−a+b).
-- ∧/∨ idem Kleene (min/max), ¬ forte. (Fato B3: p→p é tautologia.)
-- ---------------------------------------------------------------

lNot :: LukasTrit -> LukasTrit
lNot = lukasTritOfVal . (2 -) . lukasTritVal

lAnd :: LukasTrit -> LukasTrit -> LukasTrit
lAnd a b = min a b

lOr :: LukasTrit -> LukasTrit -> LukasTrit
lOr a b = max a b

lImpl :: LukasTrit -> LukasTrit -> LukasTrit
lImpl a b = lukasTritOfVal (min 2 (2 - lukasTritVal a + lukasTritVal b))

-- ---------------------------------------------------------------
-- Enumerações para testes exaustivos (espaço finito pequeno).
-- ---------------------------------------------------------------

allKleene :: [KleeneTrit]
allKleene = [KleeneTrit M, KleeneTrit Z, KleeneTrit P]

allLukas :: [LukasTrit]
allLukas = [LukasTrit M, LukasTrit Z, LukasTrit P]
