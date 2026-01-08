#include "elbo_sdk/shm_segment.h"

#include "elbo_sdk/uid.h"

#include <stdexcept>

namespace elbo_sdk {

SharedMemorySegment::SharedMemorySegment() = default;

SharedMemorySegment::~SharedMemorySegment() {
    try {
        close();
    } catch (...) {
    }
}

void SharedMemorySegment::reset_handle() {
    if (!closed_) {
        release_handle(&handle_);
        closed_ = true;
    }
}

void SharedMemorySegment::create(const std::string& name, std::size_t size) {
    reset_handle();

    if (size == 0) {
        throw std::runtime_error("shared memory size must be > 0");
    }

    if (name.empty()) {
        name_ = "pshm_" + new_uid16();
    } else {
        name_ = name;
    }

    handle_ = create_segment(name_.c_str(), size);
    if (!handle_.address || handle_.size == 0) {
        closed_ = true;
        throw std::runtime_error("failed to create shared memory segment");
    }

    closed_ = false;
}

void SharedMemorySegment::open(const std::string& name) {
    reset_handle();

    if (name.empty()) {
        throw std::runtime_error("shared memory name required when opening");
    }

    name_ = name;
    handle_ = open_segment(name_.c_str());
    if (!handle_.address || handle_.size == 0) {
        closed_ = true;
        throw std::runtime_error("failed to open shared memory segment");
    }

    closed_ = false;
}

void SharedMemorySegment::close() {
    reset_handle();
}

void SharedMemorySegment::unlink() {
    if (!name_.empty()) {
        remove_segment(name_.c_str());
    }
}

bool SharedMemorySegment::is_closed() const {
    return closed_;
}

std::string SharedMemorySegment::name() const {
    return name_;
}

std::size_t SharedMemorySegment::size() const {
    return handle_.size;
}

void* SharedMemorySegment::address() const {
    return handle_.address;
}

} // namespace elbo_sdk
