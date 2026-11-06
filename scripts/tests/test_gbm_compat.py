"""Exercise real compatibility code against a fake GBM backend without a GPU."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


class GbmCompatTests(unittest.TestCase):
    @unittest.skipUnless(shutil.which('cc'), 'A C compiler is required')
    def test_packed_geometry_mapping_validation_and_regular_buffers(self):
        source = Path(__file__).resolve().parents[1] / 'native'
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            fake = root / 'original.c'
            fake.write_text(r'''
#include "gbm_compat.h"
#include <stdlib.h>
struct buffer { uint32_t width, height, stride; };
static uint32_t format(uint32_t value) {return value;}
static void *create(int fd) {(void)fd;return (void*)1;}
static void destroy(void *ptr) {(void)ptr;}
static int allocate(void *ptr) {(void)ptr;return 123;}
static void *import_buffer(void *dev,int fd,uint32_t w,uint32_t h,uint32_t stride,uint64_t mod,uint32_t fmt) {
 (void)dev;(void)fd;(void)mod;(void)fmt;
 struct buffer *b=malloc(sizeof(*b));*b=(struct buffer){w,h,stride};return b;
}
static void map(void *ptr,int w,int h,void **addr,void **data) {
 struct buffer *b=ptr;*data=(void*)42;
 *addr=(w>0&&h>0&&(uint32_t)w<=b->width&&(uint32_t)h<=b->height)?(void*)1234:NULL;
}
static void unmap(void *ptr,void *data) {(void)ptr;(void)data;}
static struct operations ops={format,create,destroy,allocate,import_buffer,free,map,unmap};
struct operations *get_gbm_ops(void) {return &ops;}
uint32_t dimension(void *ptr,int index) {return ((uint32_t*)ptr)[index];}
''')
            subprocess.run(['cc', '-shared', '-fPIC', '-Wl,-soname,libgbm_mesa_wrapper.so', '-I', str(source), str(fake), '-o', str(root / 'libdroidpc_gbm_original.so')], check=True)
            subprocess.run(['cc', '-std=c11', '-Wall', '-Wextra', '-Werror', '-shared', '-fPIC', '-fvisibility=hidden', '-Wl,-soname,libdroidpc_gbm_wrapper.so', str(source / 'gbm_compat.c'), '-ldl', '-pthread', '-o', str(root / 'libgbm_mesa_wrapper.so')], check=True)
            check = r'''
import ctypes as c,os,sys,tempfile
root=sys.argv[1]
lib=c.CDLL(root+'/libgbm_mesa_wrapper.so');lib.get_gbm_ops.restype=c.c_void_p
slots=(c.c_void_p*8).from_address(lib.get_gbm_ops())
imp=c.CFUNCTYPE(c.c_void_p,c.c_void_p,c.c_int,c.c_uint32,c.c_uint32,c.c_uint32,c.c_uint64,c.c_uint32)(slots[4])
release=c.CFUNCTYPE(None,c.c_void_p)(slots[5])
map=c.CFUNCTYPE(None,c.c_void_p,c.c_int,c.c_int,c.POINTER(c.c_void_p),c.POINTER(c.c_void_p))(slots[6])
alloc=c.CFUNCTYPE(c.c_int,c.c_void_p)(slots[3]);assert alloc(None)==123
backend=c.CDLL(root+'/libdroidpc_gbm_original.so');backend.dimension.argtypes=[c.c_void_p,c.c_int];backend.dimension.restype=c.c_uint32
with tempfile.TemporaryFile() as file:
 fd=file.fileno();os.ftruncate(fd,2*1024*1024);os.lseek(fd,17,os.SEEK_SET)
 bo=imp(None,fd,0,1,1280,0,0x20203852);assert bo
 assert [backend.dimension(bo,i) for i in range(3)]==[4096,512,4096]
 assert os.lseek(fd,0,os.SEEK_CUR)==17
 addr=c.c_void_p();data=c.c_void_p();map(bo,983046,1,c.byref(addr),c.byref(data));assert addr.value==1234
 release(bo)
 bo=imp(None,fd,640,480,2560,0,0x34325258)
 assert [backend.dimension(bo,i) for i in range(3)]==[640,480,2560]
 map(bo,320,240,c.byref(addr),c.byref(data));assert addr.value==1234
 map(bo,641,480,c.byref(addr),c.byref(data));assert addr.value==c.c_void_p(-1).value
 release(bo)
 for size in (0,3,4096*16385):
  os.ftruncate(fd,size);assert imp(None,fd,0,1,1280,0,0x20203852) is None
 assert imp(None,-1,0,1,1280,0,0x20203852) is None
'''
            subprocess.run(['python3', '-c', check, directory], env=os.environ | {'LD_LIBRARY_PATH': directory}, check=True)
