/* Linux/glibc-only whole-process allocator probe, outside the Rust engine.
 * gcc -shared -fPIC -O2 tools/perf_alloc.c -o /tmp/be2-alloc.so
 * LD_PRELOAD=/tmp/be2-alloc.so target/fast/examples/runtime_perf
 * Counts allocator calls (including failed calls), not empty Rust Vec values.
 * Requested bytes are cumulative traffic, not retained heap or peak memory.
 */
#include <stdatomic.h>
#include <stddef.h>
#include <stdio.h>
#include <sys/resource.h>

extern void *__libc_malloc(size_t);
extern void *__libc_calloc(size_t, size_t);
extern void *__libc_realloc(void *, size_t);
static _Atomic unsigned long long calls, requested;
static void count(size_t n) {
    atomic_fetch_add_explicit(&calls, 1, memory_order_relaxed);
    atomic_fetch_add_explicit(&requested, n, memory_order_relaxed);
}
void *malloc(size_t n) { count(n); return __libc_malloc(n); }
void *calloc(size_t n, size_t size) { count(n * size); return __libc_calloc(n, size); }
void *realloc(void *p, size_t n) { count(n); return __libc_realloc(p, n); }
__attribute__((destructor)) static void report(void) {
    struct rusage usage;
    getrusage(RUSAGE_SELF, &usage);
    fprintf(stderr, "allocator_calls=%llu requested_bytes=%llu max_rss_kib=%ld\n",
            atomic_load(&calls), atomic_load(&requested), usage.ru_maxrss);
}
