/* Fixture: C file that calls into an asm routine via extern "C" naming convention */
#include "boot.h"

extern void asm_boot_routine(void);

int main(void) {
    asm_boot_routine();
    return 0;
}

void c_helper(void) {
    /* called from asm via call c_helper */
}
