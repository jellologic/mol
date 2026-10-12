// SPDX-License-Identifier: MIT OR Apache-2.0
// Apple libm oracle probe (P1.2.V2): evaluates the ten wrapped
// functions over fixed edge cases plus seeded pseudorandom inputs,
// printing exact input/output bit patterns and errno. The trace is
// checked in at corpus/ulp/apple-arm64.csv; the Linux comparator
// (mol-libsystem/tests/ulp.rs) diffs our glibc-backed wrappers
// against it.
//
// Build: clang -O2 -o /tmp/ulp-oracle tools/ulp-oracle.c
// Run:   /tmp/ulp-oracle > corpus/ulp/apple-arm64.csv
// Deterministic: fixed seed, fixed input schedule.
#include <errno.h>
#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

static uint64_t rng_state = 0x243F6A8885A308D3ULL; // fixed seed

static uint64_t next_u64(void) {
    uint64_t x = rng_state;
    x ^= x >> 12;
    x ^= x << 25;
    x ^= x >> 27;
    rng_state = x;
    return x * 0x2545F4914F6CDD1DULL;
}

static double as_double(uint64_t u) {
    double d;
    memcpy(&d, &u, 8);
    return d;
}

static uint64_t as_u64(double d) {
    uint64_t u;
    memcpy(&u, &d, 8);
    return u;
}

static void emit(const char *fn, double x, double y, int two, double r) {
    if (two)
        printf("%s,%016llx,%016llx,%016llx,%d\n", fn, (unsigned long long)as_u64(x),
               (unsigned long long)as_u64(y), (unsigned long long)as_u64(r), errno);
    else
        printf("%s,%016llx,,%016llx,%d\n", fn, (unsigned long long)as_u64(x),
               (unsigned long long)as_u64(r), errno);
}

// Edge inputs shared by all functions (NaN payloads excluded from
// comparison; presence asserts NaN-ness only).
static const uint64_t edges[] = {
    0x0000000000000000ULL, // +0
    0x8000000000000000ULL, // -0
    0x3FF0000000000000ULL, // 1
    0xBFF0000000000000ULL, // -1
    0x3FE0000000000000ULL, // 0.5
    0xBFE0000000000000ULL, // -0.5
    0x400921FB54442D18ULL, // pi
    0xC00921FB54442D18ULL, // -pi
    0x4005BF0A8B145769ULL, // e
    0x3FF0000000000001ULL, // 1+eps
    0x3FEFFFFFFFFFFFFFULL, // 1-eps
    0x0000000000000001ULL, // min subnormal
    0x000FFFFFFFFFFFFFULL, // max subnormal
    0x0010000000000000ULL, // min normal
    0x7FEFFFFFFFFFFFFFULL, // DBL_MAX
    0xFFEFFFFFFFFFFFFFULL, // -DBL_MAX
    0x7FF0000000000000ULL, // +inf
    0xFFF0000000000000ULL, // -inf
    0x7FF8000000000000ULL, // qNaN
    0xFFF8000000000000ULL, // -qNaN
    0x7FF4000000000000ULL, // sNaN-ish
    0x43E0000000000000ULL, // 2^63 (int64 edge)
    0xC3E0000000000000ULL, // -2^63
    0x3F50624DD2F1A9F8ULL, // 0.001
    0xC050000000000000ULL,  // -64
};

#define N1(f)                                          \
    do {                                               \
        for (unsigned i = 0; i < sizeof(edges) / 8; i++) { \
            double x = as_double(edges[i]);            \
            errno = 0;                                 \
            double r = f(x);                           \
            emit(#f, x, 0, 0, r);                      \
        }                                              \
        for (int k = 0; k < 1024; k++) {               \
            double x = as_double(next_u64());          \
            errno = 0;                                 \
            double r = f(x);                           \
            emit(#f, x, 0, 0, r);                      \
        }                                              \
    } while (0)

#define N2(f)                                          \
    do {                                               \
        for (unsigned i = 0; i < sizeof(edges) / 8; i++)   \
            for (unsigned j = 0; j < sizeof(edges) / 8; j++) { \
                double x = as_double(edges[i]);        \
                double y = as_double(edges[j]);        \
                errno = 0;                             \
                double r = f(x, y);                    \
                emit(#f, x, y, 1, r);                  \
            }                                          \
        for (int k = 0; k < 1024; k++) {               \
            double x = as_double(next_u64());          \
            double y = as_double(next_u64());          \
            errno = 0;                                 \
            double r = f(x, y);                        \
            emit(#f, x, y, 1, r);                      \
        }                                              \
    } while (0)

int main(void) {
    printf("func,in1,in2,out,errno\n");
    N1(sin);
    N1(cos);
    N1(exp);
    N1(log);
    N1(sqrt);
    N1(floor);
    N1(ceil);
    N1(fabs);
    N2(pow);
    N2(fmod);
    return 0;
}
