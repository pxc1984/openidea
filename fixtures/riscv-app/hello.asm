        .section .rodata
message: .asciz "Hello, RISC-V!\n"
        .text
        .globl main
main:   la      a0, message
        li      a7, 4
        ecall
        li      a0, 0
        li      a7, 93
        ecall
