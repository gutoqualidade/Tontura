import TeneruraProofs.Basic

def main : IO Unit := do
  -- Verificação de execução (a prova é verificada em tempo de build pelo kernel)
  IO.println s!"TENERURA provas: bval_shift verificada (A8). Exemplo: bval [p,m] = {Tenerura.bval [Tenerura.Trd.p, Tenerura.Trd.m]}"
