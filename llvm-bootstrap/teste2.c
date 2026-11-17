/* Cobre: mul nativo, icmp sle/sge/slt/sne, if/else, laço com dois icmp. */
int fatorial(int n) {
  int r = 1;
  int i = 2;
  while (i <= n) {
    r = r * i;
    i = i + 1;
  }
  return r;
}

int sinal(int x) {
  if (x > 0) {
    return 1;
  } else if (x < 0) {
    return -1;
  }
  return 0;
}

int main() {
  int f = fatorial(5);      /* 120 */
  int s = sinal(0 - 9);     /* -1 */
  int t = f * 2;            /* 240 */
  if (f == 120 && s == -1) {
    return t;
  }
  return 0;
}