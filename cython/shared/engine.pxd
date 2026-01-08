from libcpp.string cimport string

cdef extern from "elbo_sdk/engine_api.h" namespace "elbo_sdk":
    cdef cppclass PivotEngineApi:
        bint start(const string& engine_path, string* error_out) except +
        void stop() except +
        bint is_running() const
        string send_command(const string& command_json, string* error_out) except +
        void send_command_async(const string& command_json, string* error_out) except +
        string wait_for_response(int expected_id, string* error_out) except +

    PivotEngineApi& engine_singleton() except +

    string get_platform_id() except +
    string resolve_engine_binary_path() except +

# Provide a stable Cython-level alias to avoid name clashes with the Python def.
cdef inline string _cpp_get_platform_id() except +:
    return get_platform_id()
