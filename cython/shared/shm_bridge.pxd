from libcpp.string cimport string
from libc.stddef cimport size_t

cdef extern from "elbo_sdk/shm_segment.h" namespace "elbo_sdk":
    cdef cppclass SharedMemorySegment:
        SharedMemorySegment() except +
        void create(const string& name, size_t size) except +
        void open(const string& name) except +
        void close() except +
        void unlink() except +
        bint is_closed() const
        string name() const
        size_t size() const
        void* address() const
