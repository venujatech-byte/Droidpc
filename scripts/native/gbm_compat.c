#define _FILE_OFFSET_BITS 64
#include <dlfcn.h>
#include <errno.h>
#include <pthread.h>
#include <stdint.h>
#include <stdlib.h>
#include <stdio.h>
#include <sys/mman.h>
#include <unistd.h>

#include "gbm_compat.h"

struct packed_buffer { void *bo; uint32_t width, height; struct packed_buffer *next; };
static struct operations original, compatible;
static struct packed_buffer *buffers;
static pthread_mutex_t mutex = PTHREAD_MUTEX_INITIALIZER;
static pthread_once_t once = PTHREAD_ONCE_INIT;
static int ready;

/* R8 fallback images are allocated as 4096-byte rows, not logical bytes x 1. */
static int geometry(int fd, uint32_t *width, uint32_t *height, uint32_t *stride) {
    off_t offset = lseek(fd, 0, SEEK_CUR);
    off_t bytes = lseek(fd, 0, SEEK_END);
    if (offset >= 0) (void)lseek(fd, offset, SEEK_SET);
    else (void)lseek(fd, 0, SEEK_SET);
    // shortcut: packed buffers are capped at 64 MiB; extend for larger camera formats.
    if (bytes <= 0 || bytes % 4096 || bytes / 4096 > 16384) {
        errno = EINVAL;
        return -1;
    }
    *width = *stride = 4096;
    *height = (uint32_t)(bytes / 4096);
    return 0;
}

static void *import_buffer(void *device, int fd, uint32_t width, uint32_t height,
                           uint32_t stride, uint64_t modifier, uint32_t format) {
    int packed = format == UINT32_C(0x20203852) && height == 1;
    if (packed && geometry(fd, &width, &height, &stride)) return NULL;
    void *bo = original.import_buffer(device, fd, width, height, stride, modifier, format);
    if (!bo || !packed) return bo;
    struct packed_buffer *entry = malloc(sizeof(*entry));
    if (!entry) { original.release(bo); return NULL; }
    *entry = (struct packed_buffer){bo, width, height, NULL};
    pthread_mutex_lock(&mutex);
    entry->next = buffers;
    buffers = entry;
    pthread_mutex_unlock(&mutex);
    return bo;
}

static void release(void *bo) {
    pthread_mutex_lock(&mutex);
    struct packed_buffer **entry = &buffers;
    while (*entry && (*entry)->bo != bo) entry = &(*entry)->next;
    if (*entry) {
        struct packed_buffer *removed = *entry;
        *entry = removed->next;
        free(removed);
    }
    pthread_mutex_unlock(&mutex);
    original.release(bo);
}

static void map(void *bo, int width, int height, void **address, void **data) {
    pthread_mutex_lock(&mutex);
    for (struct packed_buffer *entry = buffers; entry; entry = entry->next) {
        if (entry->bo == bo) {
            width = (int)entry->width;
            height = (int)entry->height;
            break;
        }
    }
    pthread_mutex_unlock(&mutex);
    original.map(bo, width, height, address, data);
    if (!*address) *address = MAP_FAILED;
}

static void initialize(void) {
    void *library = dlopen("libdroidpc_gbm_original.so", RTLD_NOW | RTLD_LOCAL);
    if (!library) { fprintf(stderr, "Droidpc GBM: original wrapper load failed: %s\n", dlerror()); return; }
    struct operations *(*get)(void) = (struct operations *(*)(void))dlsym(library, "get_gbm_ops");
    if (!get) return;
    struct operations *ops = get();
    if (!ops || !ops->format || !ops->create || !ops->destroy || !ops->allocate ||
        !ops->import_buffer || !ops->release || !ops->map || !ops->unmap) return;
    original = compatible = *ops;
    compatible.import_buffer = import_buffer;
    compatible.release = release;
    compatible.map = map;
    ready = 1;
}

__attribute__((visibility("default"))) struct operations *get_gbm_ops(void) {
    pthread_once(&once, initialize);
    return ready ? &compatible : NULL;
}
