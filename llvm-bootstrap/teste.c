int multiplica(int a, int b) {
  int res = 0;
  while (b > 0) {
      res = res + a;
      b = b - 1;
  }
  return res;
}

int main() {
  int x = 4;
  int y = 3;
  int z = multiplica(x, y);
  if (z == 12) {
      return 42; // Retorna 42 se funcionou
  }
  return 0; // Retorna 0 se falhou
}