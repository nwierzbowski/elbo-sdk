#pragma once

#include <string>
#include <stdexcept>

namespace elbo_sdk {

class EngineClient {
public:
    EngineClient();
    ~EngineClient();

    EngineClient(const EngineClient&) = delete;
    EngineClient& operator=(const EngineClient&) = delete;

    void start(std::string engine_path);
    void stop();

    bool is_running() const;

    // Sends a JSON command and reads lines until a response with an "ok" field appears.
    // Returns the full JSON response line. Throws std::runtime_error on error.
    std::string send_command(const std::string& command_json);

    // Sends a JSON command without waiting for any response.
    void send_command_async(const std::string& command_json);

    // Reads lines until a response with the expected id arrives.
    // Returns the full JSON response line. Throws std::runtime_error on error.
    std::string wait_for_response(int expected_id);

    // Singleton access to the process-level client.
    static EngineClient& instance();

private:
    struct Impl;
    Impl* impl_;
};

} // namespace elbo_sdk
