/* GEP constante fora do `load`: como argumento de chamada e como alvo de `store`. */
const char msg[] = "Hello";
int tabela[4] = {1, 2, 3, 4};

int comprimento(const char *s) {
    int n = 0;
    while (s[n] != 0) {
        n = n + 1;
    }
    return n;
}

int main() {
    tabela[2] = 9;                    /* store em GEP constante */
    int a = comprimento(&msg[2]);     /* argumento GEP constante: "llo" = 3 */
    return a * 100 + tabela[2] * 10 + tabela[3];   /* 300 + 90 + 4 = 394 */
}
