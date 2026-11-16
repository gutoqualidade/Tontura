; ============================================================
; TENERURA — biblioteca mínima de usuário, entrada e saída pelo SERVIDOR
; DE CONSOLE em espaço de usuário (src/os/user/condrv.tas). A capacidade
; de envio para o endpoint do servidor fica no slot CON_SRV_SLOT.
; Incluir ao final do texto.
; ============================================================
    .equ CON_SRV_SLOT, 7
    .equ CON_OP_WRITE, 1
    .equ CON_OP_READ,  2

; putc: a2 = caractere
putc:
    mov a4, a2
    li a3, CON_OP_WRITE
    li a2, CON_SRV_SLOT
    li a1, SYS_CALL
    li t1, FOREVER
    sys
    ret

; getc: → a2 = caractere (espera até chegar um); a1 = resultado da chamada
getc:
    li a3, CON_OP_READ
    li a2, CON_SRV_SLOT
    li a1, SYS_CALL
    li t1, FOREVER
    sys
    mov a2, a3
    ret

    .include "../lib/ustd.inc"
