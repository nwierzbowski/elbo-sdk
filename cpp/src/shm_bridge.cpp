#include "shm_bridge.h"
#include <boost/interprocess/mapped_region.hpp>
#include <utility>
#include <new>

#ifdef _WIN32
#include <boost/interprocess/windows_shared_memory.hpp>
#else
#include <boost/interprocess/shared_memory_object.hpp>
#endif

using namespace boost::interprocess;

SharedMemoryHandle create_segment(const char* name, size_t size) {
#ifdef _WIN32
    // Windows: Use native shared memory (backed by paging file)
    // Note: windows_shared_memory takes size in constructor
    windows_shared_memory shm(create_only, name, read_write, size);
    
    mapped_region region(shm, read_write);
    
    auto shm_ptr = new windows_shared_memory(std::move(shm));
#else
    // Linux/macOS: Use POSIX shared memory objects
    shared_memory_object shm(create_only, name, read_write);
    shm.truncate(size);

    mapped_region region(shm, read_write);
    
    auto shm_ptr = new shared_memory_object(std::move(shm));
#endif

    auto region_ptr = new mapped_region(std::move(region));

    return SharedMemoryHandle{ 
        region_ptr->get_address(), 
        size, 
        shm_ptr,
        region_ptr
    };
}

SharedMemoryHandle open_segment(const char* name) {
#ifdef _WIN32
    windows_shared_memory shm(open_only, name, read_write);
    mapped_region region(shm, read_write);
    auto shm_ptr = new windows_shared_memory(std::move(shm));
#else
    shared_memory_object shm(open_only, name, read_write);
    mapped_region region(shm, read_write);
    auto shm_ptr = new shared_memory_object(std::move(shm));
#endif

    auto region_ptr = new mapped_region(std::move(region));

    return SharedMemoryHandle{ 
        region_ptr->get_address(), 
        region_ptr->get_size(), 
        shm_ptr,
        region_ptr
    };
}

void release_handle(SharedMemoryHandle* handle) {
    if (handle) {
        if (handle->internal_region_handle) {
            delete static_cast<mapped_region*>(handle->internal_region_handle);
            handle->internal_region_handle = nullptr;
        }
        if (handle->internal_shm_handle) {
#ifdef _WIN32
            delete static_cast<windows_shared_memory*>(handle->internal_shm_handle);
#else
            delete static_cast<shared_memory_object*>(handle->internal_shm_handle);
#endif
            handle->internal_shm_handle = nullptr;
        }
        handle->address = nullptr;
        handle->size = 0;
    }
}

// remove_segment intentionally omitted from client SDK build.
// Engine is responsible for unlinking named shared memory segments when appropriate.
