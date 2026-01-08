#pragma once

#include <cstddef>
#include <string>

#include "elbo_sdk/shm_bridge.h"

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

    void close();
    void unlink();

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
