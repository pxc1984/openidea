        .text
        .globl sum, sum32, poly

# int64_t sum(int64_t a, int64_t b)
sum:    add     a0, a0, a1
        ret

# unsigned sum32(unsigned a, unsigned b)
sum32:  addw    a0, a0, a1
        ret

# int64_t poly(int64_t a, int64_t b, int64_t c, int64_t d, int64_t x)
poly:   mul     t0, a4, a4
        mul     t0, t0, a4
        mul     t1, a0, t0
        mul     t0, a4, a4
        mul     t0, t0, a1
        add     t1, t1, t0
        mul     t0, a2, a4
        add     t1, t1, t0
        add     t1, t1, a3
        mv      a0, t1
        ret
