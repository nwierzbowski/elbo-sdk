#pragma once

#include <string>

namespace elbo_sdk {

// Higher-level API used by the Python bindings.
// This intentionally keeps the Python/Cython layer thin.
class PivotEngineApi {
public:
    PivotEngineApi();
    ~PivotEngineApi();

    PivotEngineApi(const PivotEngineApi&) = delete;
    PivotEngineApi& operator=(const PivotEngineApi&) = delete;

    bool start(const std::string& engine_path, std::string* error_out = nullptr);
    void stop();

    bool is_running() const;

    std::string send_command(const std::string& command_json, std::string* error_out = nullptr);
    void send_command_async(const std::string& command_json, std::string* error_out = nullptr);
    std::string wait_for_response(int expected_id, std::string* error_out = nullptr);

private:
    struct Impl;
    Impl* impl_;
};

// Process-wide engine singleton. This persists across Python module reloads.
PivotEngineApi& engine_singleton();

// Runtime platform identifier (e.g. "linux-x86-64", "macos-arm64").
std::string get_platform_id();

// Engine binary discovery used by the SDK (host-agnostic):
// 1) PIVOT_ENGINE_PATH env var
// 2) pivot_engine on PATH
// Returns empty string if not found.
std::string resolve_engine_binary_path();

} // namespace elbo_sdk
