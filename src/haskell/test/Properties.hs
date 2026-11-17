-- TENERURA — testes de propriedade (Fase 1)
-- Propriedades A1–A9 (aritmética) e B1–B4 (lógicas), conforme diagnóstico §5.
-- Sem sementes fixadas por padrão (QuickCheck usa seed do sistema); quando
-- reprodução for necessária, rodar com QC_SEED — registrado no relatório.
module Main (main) where

import Test.QuickCheck
import Trit
import BT
import TernArith
import TernLogic
import System.Exit (exitFailure)
import System.IO (hPutStrLn, stderr)

-- ---------------------------------------------------------------
-- Geradores
-- ---------------------------------------------------------------

-- Palavra ternária canônica com até 12 trits (limiar de exaustividade do §7)
instance Arbitrary BT where
  arbitrary = sized genBT
    where
      genBT :: Int -> Gen BT
      genBT n = do
        w <- chooseInt (0, min 12 (max 0 n))
        ts <- vectorOf w (elements [ArithTrit M, ArithTrit Z, ArithTrit P])
        return (btCanonical (BT ts))

instance Arbitrary ArithTrit where
  arbitrary = elements [ArithTrit M, ArithTrit Z, ArithTrit P]

instance Arbitrary KleeneTrit where
  arbitrary = elements allKleene

instance Arbitrary LukasTrit where
  arbitrary = elements allLukas

-- ---------------------------------------------------------------
-- A1. Unicidade de representação: para todo n, toBT n é canônico e
--     qualquer palavra canônica com o mesmo valor É toBT n.
-- ---------------------------------------------------------------
prop_A1_roundtrip :: Integer -> Bool
prop_A1_roundtrip n = btValue (toBT n) == n

prop_A1_unique :: BT -> Property
prop_A1_unique b =
  let n = btValue b
  in btCanonical b === toBT n   -- canônica com valor n é a construtora

-- ---------------------------------------------------------------
-- A3/A5. Associatividade, comutatividade, identidade, inverso.
-- ---------------------------------------------------------------
prop_A3_assoc :: BT -> BT -> BT -> Bool
prop_A3_assoc a b c = btAdd (btAdd a b) c == btAdd a (btAdd b c)

prop_A3_comm :: BT -> BT -> Bool
prop_A3_comm a b = btAdd a b == btAdd b a

prop_A5_id :: BT -> Bool
prop_A5_id a = btAdd a btZero == a && btAdd btZero a == a

prop_A5_inv :: BT -> Bool
prop_A5_inv a = btAdd a (btNeg a) == btZero

prop_A3_mul_comm :: BT -> BT -> Bool
prop_A3_mul_comm a b = btMul a b == btMul b a

prop_A4_distrib :: BT -> BT -> BT -> Bool
prop_A4_distrib a b c =
  btMul a (btAdd b c) == btAdd (btMul a b) (btMul a c)

-- ---------------------------------------------------------------
-- A6. Truncamento = round-to-nearest do múltiplo de 3^k, sem empates.
-- ---------------------------------------------------------------
prop_A6_trunc_rounds :: Integer -> Property
prop_A6_trunc_rounds n =
  forAll (chooseInt (0, 6)) $ \k ->
    let q = 3 ^ k :: Integer
        t = btValue (btTrunc k (toBT n))
        err = abs (n - q * t)
    in err <= (q - 1) `div` 2 && err * 2 < q   -- sem empates: erro estritamente < meio ulp

-- ---------------------------------------------------------------
-- A8. Shift = multiplicar por 3^k, exato.
-- ---------------------------------------------------------------
prop_A8_shift :: Integer -> Property
prop_A8_shift n =
  forAll (chooseInt (0, 8)) $ \k ->
    btValue (btShiftL k (toBT n)) == toInteger (3 ^ (k :: Int)) * n

-- ---------------------------------------------------------------
-- A9. Monotonicidade da soma: a ≤ b (em valor) ⟹ a+c ≤ b+c.
-- ---------------------------------------------------------------
prop_A9_mono :: BT -> BT -> BT -> Property
prop_A9_mono a b c =
  btValue a <= btValue b ==> btValue (btAdd a c) <= btValue (btAdd b c)

-- ---------------------------------------------------------------
-- C4. Sinal pelo trit mais significativo não nulo bate com o sinal do valor.
-- ---------------------------------------------------------------
prop_C4_sign :: BT -> Bool
prop_C4_sign b =
  let s = arithTritVal (btSign b)
      v = btValue b
  in compare s 0 == compare v 0
     && (s == 0 || arithTritVal (btSign (btNeg b)) == negate s)

-- ---------------------------------------------------------------
-- B1. De Morgan para Kleene (exaustivo: 9 casos).
-- ---------------------------------------------------------------
prop_B1_deMorgan_k :: Bool
prop_B1_deMorgan_k =
  and [ kNot (kAnd a b) == kOr (kNot a) (kNot b)
        && kNot (kOr a b) == kAnd (kNot a) (kNot b)
      | a <- allKleene, b <- allKleene ]

-- B1'. Comutatividade/associatividade/identidade/absorção do lattice K3
prop_B1_lattice_k :: Bool
prop_B1_lattice_k =
  and [ kAnd a b == kAnd b a && kOr a b == kOr b a
        && kAnd (kAnd a b) c == kAnd a (kAnd b c)
        && kOr (kOr a b) c == kOr a (kOr b c)
        && kAnd a (kOr a b) == a && kOr a (kAnd a b) == a  -- absorção
        && kNot (kNot a) == a
      | a <- allKleene, b <- allKleene, c <- allKleene ]

-- ---------------------------------------------------------------
-- B2. Em K3, ¬p ∨ p NÃO é tautologia (contraexemplo p = U) — distingue K3
--     do clássico; enquanto B3: em Ł3, p→p É tautologia (exaustivo).
-- ---------------------------------------------------------------
prop_B2_k3_law_excluded_fails :: Bool
prop_B2_k3_law_excluded_fails =
  any (\a -> kImpl (kNot a) (kOr (kNot a) a) /= KleeneTrit P
           || kOr (kNot a) a /= KleeneTrit P) allKleene
  && (kOr (kNot (KleeneTrit Z)) (KleeneTrit Z) == KleeneTrit Z)  -- o contraexemplo

prop_B3_l3_impl_id :: Bool
prop_B3_l3_impl_id = all (\a -> lImpl a a == LukasTrit P) allLukas

-- B4. As lógicas diferem de fato: implicação de Kleene falha em p→p para p=U;
--     a de Łukasiewicz não. Mesma portadora, operações diferentes.
prop_B4_logics_differ :: Bool
prop_B4_logics_differ =
  kImpl (KleeneTrit Z) (KleeneTrit Z) /= KleeneTrit P   -- K3: U → U = U
  && lImpl (LukasTrit Z) (LukasTrit Z) == LukasTrit P    -- Ł3: P → P = T

-- ---------------------------------------------------------------
-- Rodapé: execução com contagem honesta de casos
-- ---------------------------------------------------------------
check :: Testable p => String -> Int -> p -> IO Bool
check _nm n prop = do
  r <- quickCheckWithResult stdArgs { maxSuccess = n } prop
  return (case r of { Success {} -> True; _ -> False })

main :: IO ()
main = do
  let qs = 1000  -- casos por propriedade quantificada
  results <- sequence
    [ check "A1 roundtrip"       qs  prop_A1_roundtrip
    , check "A1 unicidade"       qs  prop_A1_unique
    , check "A3 assoc +"         qs  prop_A3_assoc
    , check "A3 comm +"          qs  prop_A3_comm
    , check "A3 comm ×"          qs  prop_A3_mul_comm
    , check "A4 distributiva"    qs  prop_A4_distrib
    , check "A5 identidade"      qs  prop_A5_id
    , check "A5 inverso"         qs  prop_A5_inv
    , check "A6 trunc=round"     qs  prop_A6_trunc_rounds
    , check "A8 shift=×3^k"      qs  prop_A8_shift
    , check "A9 monotonicidade"  qs  prop_A9_mono
    , check "C4 sinal nativo"    qs  prop_C4_sign
    ]
  let exhaustives =
        [ ("B1 De Morgan K3",    prop_B1_deMorgan_k)
        , ("B1 lattice K3",      prop_B1_lattice_k)
        , ("B2 K3 sem 3º excl.", prop_B2_k3_law_excluded_fails)
        , ("B3 Ł3 p→p taut.",    prop_B3_l3_impl_id)
        , ("B4 tipos distintos", prop_B4_logics_differ)
        ]
  okExh <- mapM (\(nm, p) -> check nm 1 p) exhaustives
  let ok = all id (results ++ okExh)
  if ok
    then putStrLn "TODAS AS PROPRIEDADES PASSARAM"
    else do hPutStrLn stderr "FALHAS DETECTADAS"; exitFailure
