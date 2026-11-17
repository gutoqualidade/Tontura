; ============================================================
; TENERURA — ABI do micronúcleo v1 (compartilhada por núcleo e usuário)
; docs/08-micronucleo.md §3
;
; Chamada: número em a1, argumentos em a2..a9, instrução `sys`.
; Retorno: a1 = trit de resultado
;            +1  feito
;             0  nada aconteceu (ninguém do outro lado, ou o prazo venceu)
;            −1  erro (código em a2)
;
; O núcleo NUNCA segue ponteiro do usuário: tudo trafega em registradores.
; Mensagem IPC = 7 palavras (a3..a9).
; Não há autoridade ambiente: toda chamada que toca um recurso
; compartilhado nomeia uma capacidade (slot em a2).
;
; PACIÊNCIA (em t1, nas chamadas SEND/RECV/CALL) — o sinal decide:
;   t1 < 0   espera para sempre
;   t1 = 0   não espera (só conclui se o parceiro já estiver lá)
;   t1 > 0   espera no máximo t1 passos de máquina
; ============================================================

    .equ SYS_YIELD,   0     ; cede o processador
    .equ SYS_SEND,    1     ; a2 = cap; a3..a9 = mensagem; t1 = paciência
    .equ SYS_RECV,    2     ; a2 = cap; t1 = paciência
                            ;   → a2 = selo do remetente, a3..a9 = mensagem,
                            ;     t1 = recibo de resposta (0 = não se espera resposta)
    .equ SYS_CALL,    3     ; send + espera a resposta (→ a3..a9); t1 = paciência do todo
    .equ SYS_REPLY,   4     ; a2 = recibo; a3..a9 = resposta
    .equ SYS_EXIT,    5     ; a2 = código de saída; não retorna
    .equ SYS_SLEEP,   6     ; a2 = passos de máquina a dormir
    .equ SYS_TIME,    7     ; → a2 = tempo da máquina (relógio modular)
    .equ SYS_PUTC,    8     ; a2 = cap de console; a3 = caractere (console de depuração)
    .equ SYS_TASK,    9     ; a2 = cap de tarefa; a3 = operação pelo sinal:
                            ;   − mata, + reinicia, 0 consulta
                            ;   (→ a2 = estado, a3 = causa, a4 = código de saída, a5 = geração)
    .equ SYS_IRQ_ACK, 10    ; a2 = cap de interrupção: reabilita a linha
    .equ SYS_CAP_GRANT, 11  ; a2 = cap de tarefa (alvo), a3 = slot de origem
                            ;   (nesta tarefa), a4 = slot de destino (no alvo):
                            ;   TRANSFERE a capacidade; a origem fica livre.
                            ;   Autoridade se move; nunca se copia nem se forja.
    .equ NSYS,        12

    ; códigos de erro (a2 quando a1 = −1)
    .equ E_NOSYS,     1     ; chamada inexistente
    .equ E_BADCAP,    2     ; slot inválido ou capacidade do tipo errado
    .equ E_DEADPEER,  3     ; o servidor morreu (ou foi reiniciado) antes de responder
    .equ E_NOCALLER,  4     ; recibo de resposta inválido ou vencido
    .equ E_NOSLOT,    5     ; o slot de destino da capacidade já está ocupado

    ; tipos de capacidade. Nos endpoints o SINAL é o lado do canal:
    ; positivo = lado ativo (envia), negativo = lado passivo (recebe)
    .equ CAP_NULL,    0
    .equ CAP_EP_SEND, 1
    .equ CAP_EP_RECV, -1
    .equ CAP_CONSOLE, 2     ; escrever no console de depuração do núcleo
    .equ CAP_TASK,    3     ; matar / consultar / reiniciar uma tarefa (objeto = índice)
    .equ CAP_IRQ,     4     ; reconhecer uma linha de interrupção (objeto = linha)
    .equ CAP_SLOTS,   9

    ; mensagens originadas pelo núcleo (chegam por recv, com recibo 0 e
    ; SELO NEGATIVO — o construtor de imagem reserva os selos negativos ao
    ; núcleo; tarefas só enviam com selo ≥ 0):
    ;   aviso de morte: a2 = selo configurado, a3 = id da tarefa,
    ;                   a4 = causa (0 = exit; < 0 = falha), a5 = tval,
    ;                   a6 = pc, a7 = código de saída
    ;   interrupção:    a2 = selo configurado, a3 = linha

    ; estados de tarefa devolvidos por SYS_TASK (consulta)
    .equ TS_READY,    1
    .equ TS_DEAD,     -9

    ; layout do espaço de endereços do usuário (páginas centradas)
    .equ USER_TEXT,   365   ; início da página 1 (a página 0, centrada em 0, é guarda de nulo)
    .equ USER_SP,     -364  ; pilha desce a partir do topo da página −1
    .equ FOREVER,     -1    ; paciência: para sempre

    ; convenção DESTA biblioteca de usuário (não do núcleo): a capacidade de
    ; console, quando concedida, fica no último slot
    .equ CON_SLOT,    8
