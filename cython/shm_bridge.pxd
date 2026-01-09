from libcpp.string cimport string
from libc.stddef cimport size_t

cdef extern from "shm_segment.h" namespace "elbo_sdk":
    cdef cppclass SharedMemorySegment:
        SharedMemorySegment() except +
        void create(const string& name, size_t size) except +
        void open(const string& name) except +
        void create_or_open(const string& name, size_t size, bint create_mode) except +
        void close() except +
        # unlink() not exposed - engine manages lifecycle
        bint is_closed() const
        string name() const
        size_t size() const
        void* address() const
