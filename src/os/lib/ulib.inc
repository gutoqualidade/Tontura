; ============================================================
; TENERURA — biblioteca mínima de usuário, saída pelo console de
; DEPURAÇÃO do núcleo (capacidade no slot CON_SLOT; sem ela o núcleo
; recusa com E_BADCAP e nada é impresso). Incluir ao final do texto.
; ============================================================

; putc: a2 = caractere
putc:
    mov a3, a2
    li a2, CON_SLOT
    li a1, SYS_PUTC
    sys
    ret

    .include "../lib/ustd.inc"
