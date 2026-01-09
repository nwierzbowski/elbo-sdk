#pragma once

#include <cstddef>
#include <string>

#include "shm_bridge.h"

namespace elbo_sdk {

// RAII wrapper around the platform shared-memory handle.
class SharedMemorySegment {
public:
    SharedMemorySegment();
    ~SharedMemorySegment();

    SharedMemorySegment(const SharedMemorySegment&) = delete;
    SharedMemorySegment& operator=(const SharedMemorySegment&) = delete;

    void create(const std::string& name, std::size_t size);
    void open(const std::string& name);

    // Combined create/open for Python wrapper simplicity
    void create_or_open(const std::string& name, std::size_t size, bool create_mode);

    void close();

    bool is_closed() const;

    std::string name() const;
    std::size_t size() const;
    void* address() const;

private:
    void reset_handle();

    SharedMemoryHandle handle_{};
    std::string name_;
    bool closed_ = true;
};

} // namespace elbo_sdk
