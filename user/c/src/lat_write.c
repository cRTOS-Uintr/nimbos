#include <pthread.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>
#include <unistd.h>
#include <stdint.h>

#define CYCLES_PER_SAMPLE 100
#define TOTAL_SAMPLES 20
#define USEC_PER_SEC        1000000
#define NSEC_PER_SEC        1000000000
#define DEFAULT_CLOCK       CLOCK_MONOTONIC

int do_write() {
    char c = (char)0x0E;
    return write(stdout, &c, 1) == 1;
}

static inline long tsdelta(const struct timespec* t1, const struct timespec* t2)
{
    int64_t diff = (long)USEC_PER_SEC * ((long)t1->tv_sec - (long)t2->tv_sec);
    diff += ((long)t1->tv_nsec - (long)t2->tv_nsec) / 1000;
    return diff;
}

int main() {
    int err;
    struct timespec end, start;
    long total, avg;

    for (int i = 0; i < TOTAL_SAMPLES; i++) {
        err = clock_gettime(DEFAULT_CLOCK, &start);
        for (int j = 0; j < CYCLES_PER_SAMPLE; j++) {
            do_write();
        }
        err = clock_gettime(DEFAULT_CLOCK, &end);
        long interval = tsdelta(&end, &start);
        printf("Live: latency = %ld usec", interval / CYCLES_PER_SAMPLE);
        printf("\n");
        printf("\033[A\033[2K");
        total += interval;
    }
    avg = total / TOTAL_SAMPLES;
    printf("Avg time delay per syscall: %ld usec\n", avg / CYCLES_PER_SAMPLE);
    printf("Total time delay for %d syscalls: %ld usec\n", TOTAL_SAMPLES * CYCLES_PER_SAMPLE, total);
}