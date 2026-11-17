-- TENERURA Fase 4A — artefato de verificação cruzada Rust ↔ Haskell.
--
-- Gera a lista INDEPENDENTE (lado Haskell) das palavras canônicas de largura 6:
-- para cada v ∈ [−M₆, M₆] = [−364, 364], a forma canônica `toBT v` (largura
-- variável, little-endian, sem zeros altos) é preenchida com Z no lado ALTO
-- até 6 trits e impressa em big-endian legível ('+' / '0' / '-').
--
-- Formato de cada linha: "v|dígitos6"
--
-- Consumido por src/rust/tests/alu.rs via include_str! — o teste Rust compara
-- dígito a dígito `from_i64(v,6)` contra ESTE artefato (dois codificadores
-- independentes), não uma função contra si mesma.
--
-- Execução:
--   nix-shell ~/tenerura/shell.nix --run \
--     'cd ~/tenerura/src/haskell && runghc -isrc ../../experiments/phase4a/gen_haskell_words.hs \
--      > ../../results/phase4a/haskell_canonical_width6.txt'

module Main (main) where

import BT
import Trit

width :: Int
width = 6

main :: IO ()
main = mapM_ emit [-364 .. 364]

emit :: Integer -> IO ()
emit v = putStrLn (show v ++ "|" ++ digits v)

digits :: Integer -> String
digits v = concatMap ch (reverse padded)
  where
    BT ts   = toBT v                      -- canônica, little-endian
    padded  = ts ++ replicate (width - length ts) (ArithTrit Z)
    ch t    = case arithTritVal t of
                (-1) -> "-"
                0    -> "0"
                _    -> "+"
