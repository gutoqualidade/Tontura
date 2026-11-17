-- TENERURA — palavras ternárias balanceadas (canonical form little-endian)
module BT
  ( BT(..)
  , btZero
  , btValue        -- palavra → Integer (modelo de ouro)
  , toBT           -- Integer → palavra (construtor canônico)
  , btCanonical    -- normaliza: corta trits de sinal zero nas posições altas
  , btWidth
  , btNeg
  , btSign         -- sinal via trit mais significativo NÃO nulo (O(1) pós-canônica)
  ) where

import Trit

-- | Palavra ternária balanceada, little-endian (lista indexada por 3^posição).
--   Invariante canônico: a posição mais alta com trit não nulo é a última da
--   lista; a palavra zero é a lista vazia.
newtype BT = BT [ArithTrit]
  deriving (Eq)

instance Show BT where
  show (BT ts) = concatMap (showT . arithTritVal) (reverse ts)  -- big-endian p/ humano
    where showT v = case v of { (-1) -> "-"; 0 -> "0"; _ -> "+" }

btZero :: BT
btZero = BT []

btWidth :: BT -> Int
btWidth (BT ts) = length ts

-- | Valor numérico: Σ dᵢ·3ⁱ. Ouro: calculado com Integer.
btValue :: BT -> Integer
btValue (BT ts) = go 1 ts
  where
    go _ []       = 0
    go w (d:rest) = toInteger (arithTritVal d) * w + go (w * 3) rest

-- | Normalização canônica: remove trits Z das posições mais altas.
btCanonical :: BT -> BT
btCanonical (BT ts) = BT (reverse (dropWhile (== ArithTrit Z) (reverse ts)))

-- | Construtor canônico a partir de um inteiro qualquer.
--   Algoritmo: r = n `mod` 3 (não negativo); dígito d ∈ {-1,0,1} tal que
--   n' = (n - d)/3. Caso r=2: d=-1, n'=(n+1)/3. Termina quando n=0.
toBT :: Integer -> BT
toBT = btCanonical . BT . go
  where
    go 0 = []
    go n = d : go n'
      where
        r  = n `mod` 3
        d  = arithTritOfVal (case r of { 0 -> 0; 1 -> 1; _ -> -1 })
        n' = (n - toInteger (arithTritVal d)) `div` 3

-- | Negação trit a trit (inverso aditivo, propriedade A5).
btNeg :: BT -> BT
btNeg (BT ts) = BT (map inv ts)
  where inv = ArithTrit . s3OfVal . negate . arithTritVal

-- | Sinal pelo trit mais significativo não nulo (propriedade C4).
--   Em lista little-endiana canônica, o MSB é o ÚLTIMO elemento
--   (O(n) aqui; O(1) em memória indexada a partir do topo — anotado no doc).
btSign :: BT -> ArithTrit
btSign (BT ts) = case ts of
  [] -> ArithTrit Z
  _  -> last ts
