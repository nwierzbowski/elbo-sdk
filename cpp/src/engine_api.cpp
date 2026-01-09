#include "engine_api.h"

#include "engine_client.h"

#include <boost/json.hpp>
#include <boost/process/search_path.hpp>

#include <algorithm>
#include <cstdlib>
#include <mutex>
#include <string>

namespace bp = boost::process;

namespace elbo_sdk
{

    std::string get_platform_id()
    {
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

    std::string resolve_engine_binary_path()
    {
        const char *env_path = std::getenv("PIVOT_ENGINE_PATH");
        if (env_path && *env_path)
        {
            return std::string(env_path);
        }

#if defined(_WIN32)
        const char *exe_name = "pivot_engine.exe";
#else
        const char *exe_name = "pivot_engine";
#endif

        auto found = bp::search_path(exe_name);
        if (!found.empty())
        {
            return found.string();
        }

        return {};
    }

    std::string sync_license_mode_cpp()
    {
        // Prefer the direct EngineClient-backed API for license sync.
        return send_command(R"({"id": 1, "op": "sync_license"})");
    }

    // Static/free API forwarding to the internal EngineClient singleton.
    void start(const std::string &engine_path)
    {
        EngineClient::instance().start(engine_path);
    }

    void stop()
    {
        EngineClient::instance().stop();
    }

    bool is_running()
    {
        return EngineClient::instance().is_running();
    }

    std::string send_command(const std::string &command_json)
    {
        return EngineClient::instance().send_command(command_json);
    }

    void send_command_async(const std::string &command_json)
    {
        EngineClient::instance().send_command_async(command_json);
    }

    std::string wait_for_response(int expected_id)
    {
        return EngineClient::instance().wait_for_response(expected_id);
    }

} // namespace elbo_sdk
