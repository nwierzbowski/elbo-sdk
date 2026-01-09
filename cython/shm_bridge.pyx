# distutils: language = c++
from libc.stddef cimport size_t
from cpython.buffer cimport Py_buffer

from libcpp.string cimport string

cdef class SharedMemory:
    cdef SharedMemorySegment* _seg
    cdef str _name
    cdef bint _created
    cdef bint _closed
    cdef Py_ssize_t _shape[1]

    def __cinit__(self, str name=None, bint create=False, size_t size=0):
        cdef string name_s
        self._closed = True
        self._seg = new SharedMemorySegment()

        # Use combined C++ method for simplicity
        if name is None:
            name_s = ""
        else:
            name_s = name.encode('utf-8')
        
        self._seg.create_or_open(name_s, size, create)
        self._created = create

        self._name = (<bytes>self._seg.name()).decode('utf-8', 'replace')
        self._closed = self._seg.is_closed()
        self._shape[0] = <Py_ssize_t>self._seg.size()

    def __dealloc__(self):
        try:
            if self._seg is not NULL:
                if not self._closed:
                    self._close()
        except Exception:
            pass
        if self._seg is not NULL:
            del self._seg
            self._seg = NULL

    @property
    def buf(self):
        return memoryview(self)
    
    def __getbuffer__(self, Py_buffer *buffer, int flags):
        if self._closed:
            raise ValueError("Shared memory is closed")
            
        buffer.buf = self._seg.address()
        buffer.len = self._seg.size()
        buffer.readonly = 0
        buffer.itemsize = 1
        buffer.format = b"B"
        buffer.ndim = 1
        buffer.shape = self._shape
        buffer.strides = &buffer.itemsize
        buffer.suboffsets = NULL
        buffer.internal = NULL
        buffer.obj = self

    def __releasebuffer__(self, Py_buffer *buffer):
        pass

    def _close(self):
        if not self._closed:
            self._seg.close()
            self._closed = True

    # unlink() removed - engine manages shared memory lifecycle
        
    @property
    def name(self):
        return self._name
        
    @property
    def size(self):
        return self._seg.size()
