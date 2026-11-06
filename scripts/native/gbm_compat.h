#pragma once
#include <stdint.h>
/* Eight-pointer ABI exported by the image's GBM wrapper; allocation stays unchanged. */
struct operations {
    uint32_t (*format)(uint32_t);
    void *(*create)(int);
    void (*destroy)(void *);
    int (*allocate)(void *);
    void *(*import_buffer)(void *, int, uint32_t, uint32_t, uint32_t, uint64_t, uint32_t);
    void (*release)(void *);
    void (*map)(void *, int, int, void **, void **);
    void (*unmap)(void *, void *);
};
_Static_assert(sizeof(struct operations) == 8 * sizeof(void *), "GBM wrapper ABI changed");

