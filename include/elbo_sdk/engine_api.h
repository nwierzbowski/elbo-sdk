#pragma once

#include <string>
#include <stdexcept>

namespace elbo_sdk {

// Runtime platform identifier (e.g. "linux-x86-64", "macos-arm64").
std::string get_platform_id();

// Engine binary discovery used by the SDK (host-agnostic):
// 1) PIVOT_ENGINE_PATH env var
// 2) pivot_engine on PATH
// Returns empty string if not found.
std::string resolve_engine_binary_path();

// Sync license mode from the engine (returns edition like "PRO", "STANDARD").
std::string sync_license_mode_cpp();

// C-style API surface that forwards to the internal EngineClient instance.
// These functions provide a stable, static API that Cython can call directly
// without needing a pointer to a singleton object.
void start(const std::string& engine_path = "");
void stop();
bool is_running();

std::string send_command(const std::string& command_json);
void send_command_async(const std::string& command_json);
std::string wait_for_response(int expected_id);

} // namespace elbo_sdk
