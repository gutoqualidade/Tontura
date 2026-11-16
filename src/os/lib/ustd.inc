; ============================================================
; TENERURA — rotinas de usuário independentes do caminho de saída.
; Precisam de uma rotina `putc` (a2 = caractere) já definida: quem inclui
; escolhe ulib.inc (console de depuração do núcleo) ou ulib_drv.inc
; (servidor de console em espaço de usuário).
; ============================================================

; puts: a2 = endereço de texto terminado em 0 (um caractere por palavra)
puts:
    push lr
    push s1
    mov s1, a2
1:  ld a2, 0(s1)
    bz a2, 2f
    call putc
    addi s1, s1, 1
    j 1b
2:  pop s1
    pop lr
    ret

; putdec: a2 = valor; imprime em decimal com sinal.
; A instrução DIV é CENTRADA (resto em [−5, 5]); o resto negativo é
; corrigido pedindo emprestado ao quociente.
putdec:
    push lr
    push s1
    push s2
    push s3
    mov s1, a2
    bnn s1, 1f
    li a2, '-'
    call putc
    neg s1, s1
1:  li s2, 0
    li s3, 10
2:  div t1, s1, s3
    mul t2, t1, s3
    sub t2, s1, t2
    bnn t2, 3f
    addi t2, t2, 10
    addi t1, t1, -1
3:  addi t2, t2, '0'
    push t2
    addi s2, s2, 1
    mov s1, t1
    bnz s1, 2b
4:  pop a2
    call putc
    addi s2, s2, -1
    bnz s2, 4b
    pop s3
    pop s2
    pop s1
    pop lr
    ret

; exit: a2 = código; não retorna
exit:
    li a1, SYS_EXIT
    sys
