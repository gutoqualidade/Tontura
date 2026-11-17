-- TENERURA — portador físico {-1,0,+1} e semânticas tipadas
-- Princípio B4 do diagnóstico: aritmética, Kleene e Łukasiewicz compartilham
-- o portador S3, mas são TIPOS DISTINTOS. Conversões são explícitas e totais.
module Trit
  ( S3(..)
  , s3Val, s3OfVal
  , ArithTrit(..)
  , arithTritVal, arithTritOfVal
  , KleeneTrit(..)
  , kleeneTritVal
  , LukasTrit(..)
  , lukasTritVal, lukasTritOfVal
  ) where

-- | Portador físico: os três estados distinguíveis.
--   Convenção: M = -1 (nível inferior), Z = 0 (referência), P = +1 (nível superior).
--   IMPORTANTE: aqui Z é apenas o portador. Significados vivem nos newtypes abaixo.
data S3 = M | Z | P
  deriving (Eq, Ord, Show, Enum, Bounded)

s3Val :: S3 -> Int
s3Val M = -1
s3Val Z =  0
s3Val P =  1

-- | Total: negativo → M, zero → Z, positivo → P. (Não é inverso de s3Val em
--   geral: s3OfVal 2 = P. Uso restrito a valores em {-1,0,1}.)
s3OfVal :: Int -> S3
s3OfVal v
  | v < 0     = M
  | v == 0    = Z
  | otherwise = P

-- | Semântica ARITMÉTICA: dígito ternário balanceado.
--   Sem instância Num: a soma de dois trits NÃO é um trit (precisa carry).
--   A "igualdade" entre dígito e inteiro é decidida por arithTritVal/arithTritOfVal.
newtype ArithTrit = ArithTrit S3
  deriving (Eq, Ord, Show)

arithTritVal :: ArithTrit -> Int
arithTritVal (ArithTrit s) = s3Val s

arithTritOfVal :: Int -> ArithTrit
arithTritOfVal = ArithTrit . s3OfVal

-- | Semântica LÓGICA de Kleene (K3): F, U (indeterminado), T.
--   Order F ≤ U ≤ T é a ordem do lattice (min/max = ∧/∨).
newtype KleeneTrit = KleeneTrit S3
  deriving (Eq, Ord, Show)

kleeneTritVal :: KleeneTrit -> Int
kleeneTritVal (KleeneTrit s) = s3Val s

-- | Semântica LÓGICA de Łukasiewicz (Ł3): F, P (possível), T.
--   Tipo distinto de KleeneTrit de propósito: a implicação difere (B4).
newtype LukasTrit = LukasTrit S3
  deriving (Eq, Ord, Show)

lukasTritVal :: LukasTrit -> Int
lukasTritVal (LukasTrit s) = s3Val s

lukasTritOfVal :: Int -> LukasTrit
lukasTritOfVal = LukasTrit . s3OfVal
