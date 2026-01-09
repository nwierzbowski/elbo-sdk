#include "elbo_sdk/engine_client.h"
#include "elbo_sdk/engine_api.h"

#include <boost/json.hpp>
#include <boost/process.hpp>

#include <chrono>
#include <memory>
#include <mutex>
#include <sstream>
#include <string>

namespace bp = boost::process;
namespace bj = boost::json;

namespace elbo_sdk {

struct EngineClient::Impl {
    std::mutex mu;
    std::unique_ptr<bp::child> child;
    std::unique_ptr<bp::opstream> in;
    std::unique_ptr<bp::ipstream> out;

    bool is_running_unsafe() const {
        return child && child->running();
    }

    static std::string ensure_newline(std::string s) {
        if (!s.empty() && s.back() != '\n') {
            s.push_back('\n');
        }
        return s;
    }

    bool write_line_unsafe(const std::string& line, std::string* error_out) {
        if (!is_running_unsafe() || !in) {
            if (error_out) *error_out = "engine not running";
            return false;
        }
        (*in) << line;
        in->flush();
        if (!(*in)) {
            if (error_out) *error_out = "failed writing to engine stdin";
            return false;
        }
        return true;
    }

    bool read_line_unsafe(std::string* line_out, std::string* error_out) {
        if (!is_running_unsafe() || !out) {
            if (error_out) *error_out = "engine not running";
            return false;
        }
        std::string line;
        if (!std::getline(*out, line)) {
            if (error_out) *error_out = "failed reading from engine stdout";
            return false;
        }
        *line_out = line;
        return true;
    }
};

EngineClient::EngineClient() : impl_(new Impl()) {}

EngineClient::~EngineClient() {
    stop();
    delete impl_;
    impl_ = nullptr;
}

void EngineClient::start(std::string engine_path) {
    std::lock_guard<std::mutex> lock(impl_->mu);

    if (impl_->is_running_unsafe()) {
        return;
    }

    std::string resolved = engine_path;
    if (resolved.empty()) {
        resolved = resolve_engine_binary_path();
    }

    if (resolved.empty()) {
        throw std::runtime_error("engine path not provided and could not be resolved");
    }

    try {
        impl_->in = std::make_unique<bp::opstream>();
        impl_->out = std::make_unique<bp::ipstream>();

        impl_->child = std::make_unique<bp::child>(
            resolved,
            bp::std_in < *impl_->in,
            bp::std_out > *impl_->out
        );

        if (!impl_->child->running()) {
            impl_->child.reset();
            impl_->in.reset();
            impl_->out.reset();
            throw std::runtime_error("engine process did not start");
        }

    } catch (const std::exception& e) {
        impl_->child.reset();
        impl_->in.reset();
        impl_->out.reset();
        throw std::runtime_error(e.what());
    }
}

void EngineClient::stop() {
    std::lock_guard<std::mutex> lock(impl_->mu);

    if (!impl_->child) {
        return;
    }

    try {
        if (impl_->is_running_unsafe()) {
            std::string err;
            (void)impl_->write_line_unsafe("__quit__\n", &err);

            // Best-effort graceful shutdown.
            if (!impl_->child->wait_for(std::chrono::milliseconds(2000))) {
                impl_->child->terminate();
                if (!impl_->child->wait_for(std::chrono::milliseconds(1000))) {
                    impl_->child->terminate();
                }
            }
        }
    } catch (...) {
        // Fall through to cleanup.
    }

    try {
        if (impl_->child && impl_->child->running()) {
            impl_->child->terminate();
            impl_->child->wait();
        }
    } catch (...) {
    }

    impl_->child.reset();
    impl_->in.reset();
    impl_->out.reset();
}

bool EngineClient::is_running() const {
    std::lock_guard<std::mutex> lock(impl_->mu);
    return impl_->is_running_unsafe();
}

std::string EngineClient::send_command(const std::string& command_json) {
    std::lock_guard<std::mutex> lock(impl_->mu);

    if (!impl_->is_running_unsafe()) {
        throw std::runtime_error("Engine process not started or has terminated.");
    }

    std::string line = Impl::ensure_newline(command_json);
    std::string err;
    if (!impl_->write_line_unsafe(line, &err)) {
        throw std::runtime_error(err);
    }

    while (true) {
        std::string resp_line;
        if (!impl_->read_line_unsafe(&resp_line, &err)) {
            throw std::runtime_error(err);
        }
        if (resp_line.empty()) {
            continue;
        }

        bj::error_code ec;
        bj::value v = bj::parse(resp_line, ec);
        if (ec || !v.is_object()) {
            // Ignore malformed/non-object lines; keep reading.
            continue;
        }

        const bj::object& obj = v.as_object();
        if (obj.if_contains("ok") != nullptr) {
            return resp_line;
        }
    }
}

void EngineClient::send_command_async(const std::string& command_json) {
    std::lock_guard<std::mutex> lock(impl_->mu);

    if (!impl_->is_running_unsafe()) {
        throw std::runtime_error("Engine process not started or has terminated.");
    }

    std::string line = Impl::ensure_newline(command_json);
    std::string err;
    if (!impl_->write_line_unsafe(line, &err)) {
        throw std::runtime_error(err);
    }
}

std::string EngineClient::wait_for_response(int expected_id) {
    std::lock_guard<std::mutex> lock(impl_->mu);

    if (!impl_->is_running_unsafe()) {
        throw std::runtime_error("Engine process not started or has terminated.");
    }

    while (true) {
        std::string resp_line;
        std::string err;
        if (!impl_->read_line_unsafe(&resp_line, &err)) {
            throw std::runtime_error(err);
        }
        if (resp_line.empty()) {
            continue;
        }

        bj::error_code ec;
        bj::value v = bj::parse(resp_line, ec);
        if (ec || !v.is_object()) {
            continue;
        }

        const bj::object& obj = v.as_object();
        auto id_it = obj.if_contains("id");
        if (!id_it) {
            continue;
        }

        int id = -1;
        if (id_it->is_int64()) {
            id = static_cast<int>(id_it->as_int64());
        } else if (id_it->is_uint64()) {
            id = static_cast<int>(id_it->as_uint64());
        }

        if (id == expected_id) {
            return resp_line;
        }
    }
}

EngineClient& EngineClient::instance() {
    static EngineClient instance;
    return instance;
}

} // namespace elbo_sdk
