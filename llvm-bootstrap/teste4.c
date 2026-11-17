/* Fase 3 estendida: string global, array global, ponteiro como parâmetro, índice variável. */
const char msg[] = "Hello";
int tabela[4] = {1, 2, 3, 4};

int comprimento(const char *s) {
    int n = 0;
    while (s[n] != 0) {
        n = n + 1;
    }
    return n;
}

int soma(int *v, int n) {
    int t = 0;
    int i = 0;
    while (i < n) {
        t = t + v[i];
        i = i + 1;
    }
    return t;
}

int main() {
    int local[4];
    int i = 0;
    while (i < 4) {
        local[i] = i * 3;          /* 0 3 6 9 */
        i = i + 1;
    }
    int a = comprimento(msg);      /* 5 */
    int b = soma(tabela, 4);       /* 10 */
    int c = soma(local, 4);        /* 18 */
    int d = msg[1];                /* 'e' = 101 */
    return ((a * 100 + b) * 100 + c) * 1000 + d;   /* 51018101 */
}
