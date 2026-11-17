-- TENERURA — aritmética ternária balanceada: soma, produto, truncamento, shift
module TernArith
  ( btAdd, btMul
  , btTrunc      -- truncar k trits baixos (A6: round-to-nearest sem empates)
  , btShiftL     -- multiplicar por 3^k (A8)
  ) where

import Trit
import BT

-- | Soma coluna a coluna com carry ∈ {-1,0,1} (propriedade A2).
--   Coluna: d₁ + d₂ + carry ∈ [-2,2]; se |s|>1, dígito = s ∓ 3 e carry = ±1.
--   Estratégia: preencher ambas as palavras até n = 1 + max width (garante
--   coluna para o carry final), somar coluna a coluna, canonizar no fim.
btAdd :: BT -> BT -> BT
btAdd (BT xs) (BT ys) = btCanonical (BT (cols 0 xs' ys'))
  where
    n   = 1 + max (length xs) (length ys)
    xs' = xs ++ replicate (n - length xs) (ArithTrit Z)
    ys' = ys ++ replicate (n - length ys) (ArithTrit Z)

    cols _ [] _ = []          -- inalcançável (listas de mesmo comprimento)
    cols _ _ [] = []
    cols c (x:xt) (y:yt) = dOf d : cols c' xt yt
      where
        s  = arithTritVal x + arithTritVal y + c
        d  = if s > 1 then s - 3 else if s < (-1) then s + 3 else s
        c' = if s > 1 then 1 else if s < (-1) then (-1) else 0

    dOf v = ArithTrit (case compare v 0 of { LT -> M; EQ -> Z; GT -> P })

-- | Produto por shift-add sobre os dígitos do multiplicador (A4 alvo de prova).
btMul :: BT -> BT -> BT
btMul a (BT bs) = foldr step btZero (zip [0 ..] bs)
  where
    step (i, d) acc
      | arithTritVal d == 0 = acc
      | arithTritVal d == 1 = btAdd (btShiftL i a) acc
      | otherwise           = btAdd (btShiftL i (btNeg a)) acc

-- | A8: shift de k posições = multiplicar por 3^k, exato.
btShiftL :: Int -> BT -> BT
btShiftL k (BT ts) = btCanonical (BT (replicate k (ArithTrit Z) ++ ts))

-- | A6: descartar os k trits menos significativos. No balanceado, isso É
--   arredondamento ao múltiplo de 3^k mais próximo, sem empates
--   (erro ≤ (3^k−1)/2 < 3^k/2). Propriedade testada em Properties.hs.
btTrunc :: Int -> BT -> BT
btTrunc k (BT ts) = btCanonical (BT (drop k ts))
