#include "elbo_sdk/engine_api.h"

#include "elbo_sdk/engine_client.h"

#include <boost/process/search_path.hpp>

#include <cstdlib>
#include <mutex>
#include <string>

namespace bp = boost::process;

namespace elbo_sdk {

struct PivotEngineApi::Impl {
    // EngineClient already has its own internal mutex, but we want to keep the
    // higher-level API conservative and stable even if its internals change.
    mutable std::mutex mu;
    EngineClient client;
};

PivotEngineApi::PivotEngineApi() : impl_(new Impl()) {}

PivotEngineApi::~PivotEngineApi() {
    try {
        stop();
    } catch (...) {
    }
    delete impl_;
    impl_ = nullptr;
}

bool PivotEngineApi::start(const std::string& engine_path, std::string* error_out) {
    std::lock_guard<std::mutex> lock(impl_->mu);

    std::string resolved = engine_path;
    if (resolved.empty()) {
        resolved = resolve_engine_binary_path();
    }

    if (resolved.empty()) {
        if (error_out) {
            *error_out = "engine path not provided and could not be resolved";
        }
        return false;
    }

    return impl_->client.start(resolved, error_out);
}

void PivotEngineApi::stop() {
    std::lock_guard<std::mutex> lock(impl_->mu);
    impl_->client.stop();
}

bool PivotEngineApi::is_running() const {
    std::lock_guard<std::mutex> lock(impl_->mu);
    return impl_->client.is_running();
}

std::string PivotEngineApi::send_command(const std::string& command_json, std::string* error_out) {
    std::lock_guard<std::mutex> lock(impl_->mu);
    return impl_->client.send_command(command_json, error_out);
}

void PivotEngineApi::send_command_async(const std::string& command_json, std::string* error_out) {
    std::lock_guard<std::mutex> lock(impl_->mu);
    impl_->client.send_command_async(command_json, error_out);
}

std::string PivotEngineApi::wait_for_response(int expected_id, std::string* error_out) {
    std::lock_guard<std::mutex> lock(impl_->mu);
    return impl_->client.wait_for_response(expected_id, error_out);
}

PivotEngineApi& engine_singleton() {
    static PivotEngineApi instance;
    return instance;
}

std::string get_platform_id() {
    std::string system;
#if defined(_WIN32)
    system = "windows";
#elif defined(__APPLE__)
    system = "macos";
#elif defined(__linux__)
    system = "linux";
#else
    system = "unknown";
#endif

    std::string arch;
#if defined(__x86_64__) || defined(_M_X64) || defined(__amd64__) || defined(_M_AMD64)
    arch = "x86-64";
#elif defined(__aarch64__) || defined(_M_ARM64) || defined(__arm64__)
    arch = "arm64";
#else
    arch = "unknown";
#endif

    return system + "-" + arch;
}

std::string resolve_engine_binary_path() {
    const char* env_path = std::getenv("PIVOT_ENGINE_PATH");
    if (env_path && *env_path) {
        return std::string(env_path);
    }

#if defined(_WIN32)
    const char* exe_name = "pivot_engine.exe";
#else
    const char* exe_name = "pivot_engine";
#endif

    auto found = bp::search_path(exe_name);
    if (!found.empty()) {
        return found.string();
    }

    return {};
}

} // namespace elbo_sdk
