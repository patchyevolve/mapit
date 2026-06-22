; Fixture: Assembly file defining asm_boot_routine, calls back into C's c_helper

.globl asm_boot_routine

asm_boot_routine:
    ; Setup stack frame
    push rbp
    mov rbp, rsp

    ; Call C helper
    call c_helper

    ; Return
    pop rbp
    ret
