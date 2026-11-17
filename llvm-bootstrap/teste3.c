int main() {
    int arr[3];
    arr[0] = 10;
    arr[1] = 20;
    arr[2] = 30;
    
    int soma = arr[0] + arr[1] + arr[2];
    
    // Testar se o GEP e o load funcionaram
    if (soma == 60) {
        return 42;
    }
    return 0;
}
