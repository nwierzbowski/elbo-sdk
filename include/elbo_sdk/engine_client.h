#pragma once

#include <string>

namespace elbo_sdk {

class EngineClient {
public:
    EngineClient();
    ~EngineClient();

    EngineClient(const EngineClient&) = delete;
    EngineClient& operator=(const EngineClient&) = delete;

    bool start(const std::string& engine_path, std::string* error_out = nullptr);
    void stop();

    bool is_running() const;

    // Sends a JSON command and reads lines until a response with an "ok" field appears.
    // Returns the full JSON response line. Returns empty string on error.
    std::string send_command(const std::string& command_json, std::string* error_out = nullptr);

    // Sends a JSON command without waiting for any response.
    void send_command_async(const std::string& command_json, std::string* error_out = nullptr);

    // Reads lines until a response with the expected id arrives.
    // Returns the full JSON response line. Returns empty string on error.
    std::string wait_for_response(int expected_id, std::string* error_out = nullptr);

private:
    struct Impl;
    Impl* impl_;
};

} // namespace elbo_sdk
