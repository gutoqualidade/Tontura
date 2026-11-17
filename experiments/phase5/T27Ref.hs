-- TENERURA Fase 5 — SEGUNDA implementação, independente, da semântica de
-- instruções da ISA T27 (docs/07-isa-t27.md), para verificação cruzada do
-- emulador Rust.
--
-- Independência deliberada em relação ao emulador (src/t27):
--   * a palavra é uma LISTA DE 27 DÍGITOS balanceados (biblioteca da Fase 1:
--     BT, TernArith, TernLogic), não um inteiro de máquina;
--   * campos de instrução saem por FATIAMENTO da lista de dígitos, não por
--     divisão/resto;
--   * o wrap é "ficar com os 27 dígitos baixos"; o carry de ADDC é o dígito
--     28 da soma; MULH são os dígitos 27..53 do produto;
--   * o deslocamento à direita é o btTrunc (A6) da Fase 1;
--   * a divisão centrada sai da DEFINIÇÃO (inteiro mais próximo; empate →
--     o mais próximo de zero), por busca entre os dois candidatos.
-- A tabela de opcodes foi transcrita do documento, não do código Rust.
--
-- Entrada (stdin): linhas produzidas por `isa_cases` (src/t27/src/bin):
--   pc | 27 regs | maddr mval | palavra || classe | pc' | 27 regs' | mval'
-- Saída: resumo; código de saída ≠ 0 se houver divergência.
--
-- Execução: ver tools/isa-crosscheck.sh
module Main (main) where

import BT
import TernArith
import TernLogic
import Trit
import System.Exit (exitFailure, exitSuccess)
import System.IO (hPutStrLn, stderr)

-- ── palavra como 27 dígitos ──

type Digits = [Int]   -- little-endian, comprimento 27, cada um em {-1,0,1}

width :: Int
width = 27

digitsOf :: Integer -> [Int]      -- dígitos balanceados, sem limite de largura
digitsOf v = let BT ts = toBT v in map arithTritVal ts

pad :: Int -> [Int] -> [Int]
pad n ds = take n (ds ++ repeat 0)

toWord :: Integer -> Digits       -- 27 dígitos BAIXOS = wrap balanceado
toWord = pad width . digitsOf

valOf :: [Int] -> Integer
valOf ds = btValue (BT (map arithTritOfVal ds))

wrap :: Integer -> Integer
wrap = valOf . toWord

btOf :: Integer -> BT
btOf = toBT

-- soma pela biblioteca de dígitos (não pela soma de Integer)
addV :: Integer -> Integer -> Integer
addV a b = btValue (btAdd (btOf a) (btOf b))

-- ── decodificação por fatiamento ──

field :: Int -> Int -> Digits -> Integer
field lo len ds = valOf (take len (drop lo ds))

data Fmt = R | I | J | N deriving (Eq, Show)

data Op = Add | Sub | Addc | Mul | Mulh | Div | Rem | Cmp | Sh | Kmin | Kmax
        | Addi | Shi | Lui | Ld | St
        | Beq | Bne | Blt | Bge | Bgt | Ble | Jal | Jalr
        | Sys | Brk
        | Eret | Wfi | Csrr | Csrw | Csrx | Tlbi
        deriving (Eq, Show)

-- transcrição da tabela de docs/07-isa-t27.md §4
opTable :: [(Integer, Op, Fmt)]
opTable =
  [ (1, Add, R), (2, Sub, R), (3, Addc, R), (4, Mul, R), (5, Mulh, R)
  , (6, Div, R), (7, Rem, R), (8, Cmp, R), (9, Sh, R), (10, Kmin, R), (11, Kmax, R)
  , (12, Addi, I), (13, Shi, I), (14, Lui, I), (15, Ld, I), (16, St, I)
  , (17, Beq, I), (18, Bne, I), (19, Blt, I), (20, Bge, I), (21, Bgt, I), (22, Ble, I)
  , (23, Jal, J), (24, Jalr, I), (25, Sys, N), (26, Brk, N)
  , (-1, Eret, N), (-2, Wfi, N), (-3, Csrr, I), (-4, Csrw, I), (-5, Csrx, I), (-6, Tlbi, N)
  ]

data Instr = Instr { iOp :: Op, iA :: Integer, iB :: Integer, iC :: Integer, iImm :: Integer }

-- Nothing = instrução ilegal (decodificação estrita)
decode :: Digits -> Maybe Instr
decode ds = do
  (op, fmt) <- lookup3 (field 22 5 ds)
  let a = field 19 3 ds
      b = field 16 3 ds
      c = field 13 3 ds
  ins <- case fmt of
    R | field 0 13 ds == 0 -> Just (Instr op a b c 0)
    I                      -> Just (Instr op a b 0 (field 0 16 ds))
    J                      -> Just (Instr op a 0 0 (field 0 19 ds))
    N | field 0 22 ds == 0 -> Just (Instr op 0 0 0 0)
    _                      -> Nothing
  -- restrições específicas
  case op of
    Lui  | iB ins /= 0 || abs (iImm ins) > (3 ^ (11 :: Int) - 1) `div` 2 -> Nothing
    Csrr | iB ins /= 0 -> Nothing
    Csrw | iA ins /= 0 -> Nothing
    _ -> Just ins
  where
    lookup3 code = case [ (o, f) | (cd, o, f) <- opTable, cd == code ] of
      (x : _) -> Just x
      []      -> Nothing

privileged :: Op -> Bool
privileged op = op `elem` [Eret, Wfi, Csrr, Csrw, Csrx, Tlbi]

-- ── semântica ──

-- deslocamento com sinal: k > 0 sobe dígitos (os de topo saem);
-- k < 0 descarta |k| dígitos baixos (A6); |k| ≥ 27 dá zero
shift :: Integer -> Integer -> Integer
shift v k
  | k == 0                 = v
  | abs k >= 27            = 0
  | k > 0                  = wrap (btValue (btShiftL (fromInteger k) (btOf v)))
  | otherwise              = btValue (btTrunc (fromInteger (negate k)) (btOf v))

-- divisão centrada pela definição: q minimiza |a − q·b|; empate → menor |q|
divRem :: Integer -> Integer -> (Integer, Integer)
divRem a b
  | b == 0    = (0, a)
  | otherwise = (q, a - q * b)
  where
    f  = a `div` b                      -- piso: os candidatos são f e f+1
    err x = abs (a - x * b)
    q | err f < err (f + 1) = f
      | err f > err (f + 1) = f + 1
      | otherwise           = if abs f < abs (f + 1) then f else f + 1

kleene :: (KleeneTrit -> KleeneTrit -> KleeneTrit) -> Integer -> Integer -> Integer
kleene f a b = valOf (zipWith g (toWord a) (toWord b))
  where
    g x y = kleeneTritVal (f (KleeneTrit (s3OfVal x)) (KleeneTrit (s3OfVal y)))

sgn :: Integer -> Integer
sgn = signum

data State = State { sPc :: Integer, sRegs :: [Integer], sMem :: Integer }
  deriving (Eq, Show)

reg :: State -> Integer -> Integer
reg st r = if r == 0 then 0 else sRegs st !! fromInteger (r + 13)

setReg :: Integer -> Integer -> State -> State
setReg r v st
  | r == 0    = st
  | otherwise = st { sRegs = [ if i == r + 13 then v else x | (i, x) <- zip [0 ..] (sRegs st) ] }

-- executa uma instrução de usuário; `maddr` é a única célula de memória do caso
exec :: Integer -> Instr -> State -> State
exec maddr (Instr op a b c imm) st = case op of
  Add  -> next (setReg a (wrap (addV rb rc)) st)
  Sub  -> next (setReg a (wrap (addV rb (negate rc))) st)
  Addc -> next (setReg a (carryOf (addV rb rc)) st)
  Mul  -> next (setReg a (wrap prod) st)
  Mulh -> next (setReg a (valOf (take width (drop width (pad (2 * width) (digitsOf prod))))) st)
  Div  -> next (setReg a (fst (divRem rb rc)) st)
  Rem  -> next (setReg a (snd (divRem rb rc)) st)
  Cmp  -> next (setReg a (sgn (rb - rc)) st)
  Sh   -> next (setReg a (shift rb rc) st)
  Kmin -> next (setReg a (kleene kAnd rb rc) st)
  Kmax -> next (setReg a (kleene kOr rb rc) st)
  Addi -> next (setReg a (wrap (addV rb imm)) st)
  Shi  -> next (setReg a (shift rb imm) st)
  Lui  -> next (setReg a (btValue (btShiftL 16 (btOf imm))) st)
  Ld   -> if wrap (addV rb imm) == maddr
            then next (setReg a (sMem st) st)
            else error "LD fora da célula do caso"
  St   -> if wrap (addV rb imm) == maddr
            then next st { sMem = ra }
            else error "ST fora da célula do caso"
  Beq  -> branch (ra == rb)
  Bne  -> branch (ra /= rb)
  Blt  -> branch (ra < rb)
  Bge  -> branch (ra >= rb)
  Bgt  -> branch (ra > rb)
  Ble  -> branch (ra <= rb)
  Jal  -> (setReg a (wrap (addV pc 1)) st) { sPc = wrap (addV pc imm) }
  Jalr -> (setReg a (wrap (addV pc 1)) st) { sPc = wrap (addV rb imm) }
  _    -> error ("operação fora do escopo: " ++ show op)
  where
    pc = sPc st
    ra = reg st a
    rb = reg st b
    rc = reg st c
    prod = btValue (btMul (btOf rb) (btOf rc))
    next s = s { sPc = wrap (addV pc 1) }
    branch t = st { sPc = if t then wrap (addV pc imm) else wrap (addV pc 1) }
    -- carry = valor dos dígitos a partir do 28º
    carryOf s = valOf (drop width (pad (width + 2) (digitsOf s)))

-- ── leitura dos casos e comparação ──

splitOn :: String -> String -> [String]
splitOn sep s = go s
  where
    n = length sep
    go str = case breakOn str of
      (h, Nothing) -> [h]
      (h, Just t)  -> h : go t
    breakOn [] = ("", Nothing)
    breakOn str@(ch : rest)
      | take n str == sep   = ("", Just (drop n str))
      | otherwise           = let (h, t) = breakOn rest in (ch : h, t)

nums :: String -> [Integer]
nums = map read . words

data Verdict = Same | Skipped | Diff String

check :: String -> Verdict
check line =
  case splitOn " || " line of
    [before, after] ->
      case (map nums (splitOn " | " before), splitOn " | " after) of
        ([[pc], regs, [maddr, mval], [word]], [cls, pcS, regsS, mvalS]) ->
          let st0   = State pc regs mval
              want  = State (read pcS) (nums regsS) (read mvalS)
              ds    = toWord word
          in if valOf ds /= word then Diff "palavra fora da janela" else
             case (cls, decode ds) of
               ("I", Nothing)  -> Same
               ("I", Just i)   -> Diff ("Rust: ilegal; Haskell decodificou " ++ show (iOp i))
               (_,   Nothing)  -> Diff "Rust executou; Haskell: ilegal"
               ("P", Just i)   -> if privileged (iOp i) then Skipped else Diff "classe P mas não privilegiada"
               ("S", Just i)   -> if iOp i == Sys then Skipped else Diff "classe S mas não é sys"
               ("B", Just i)   -> if iOp i == Brk then Skipped else Diff "classe B mas não é brk"
               ("X", Just i)   -> if iOp i `elem` [Ld, St] then Skipped else Diff "classe X mas não é LD/ST"
               ("E", Just i)
                 | privileged (iOp i) -> Diff "classe E mas privilegiada"
                 | otherwise ->
                     let got = exec maddr i st0
                     in if got == want then Same
                        else Diff (show (iOp i) ++ ": Haskell " ++ brief st0 got ++ " ≠ Rust " ++ brief st0 want)
               _ -> Diff ("classe desconhecida " ++ cls)
        _ -> Diff "linha malformada"
    _ -> Diff "linha malformada"

-- só o que mudou em relação ao estado inicial
brief :: State -> State -> String
brief s0 s = "pc=" ++ show (sPc s) ++ " mem=" ++ show (sMem s) ++ " regs≠" ++
  show [ (i - 13, x) | (i, x, y) <- zip3 [0 :: Integer ..] (sRegs s) (sRegs s0), x /= y ]

main :: IO ()
main = do
  input <- getContents
  let ls       = filter (not . null) (lines input)
      results  = zip [1 :: Int ..] (map check ls)
      diffs    = [ (n, d) | (n, Diff d) <- results ]
      same     = length [ () | (_, Same) <- results ]
      skipped  = length [ () | (_, Skipped) <- results ]
  mapM_ (\(n, d) -> hPutStrLn stderr ("DIVERGÊNCIA no caso " ++ show n ++ ": " ++ d)) (take 20 diffs)
  putStrLn ("casos: " ++ show (length ls) ++ "  iguais: " ++ show same
            ++ "  só classificados (privilegiadas/sys/brk): " ++ show skipped
            ++ "  divergentes: " ++ show (length diffs))
  if null diffs then exitSuccess else exitFailure
