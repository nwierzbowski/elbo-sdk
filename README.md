# elbo-sdk

Platform-independent SDK for interacting with `pivot_engine` via JSON-over-stdio and shared memory.

## Design boundary

- **`elbo-sdk`**: engine IPC + shared memory primitives (no Blender API usage)
- **`blender-bridge`**: all `bpy`/`mathutils` work (scene traversal, mesh extraction, transform application)

## Engine binary location

The SDK does **not** assume a Blender extension layout.

Resolution order used by `elbo_sdk.engine`:
1. Pass `engine_path=` to `get_engine_communicator(engine_path=...)`
2. Set environment variable `PIVOT_ENGINE_PATH`
3. Ensure `pivot_engine` is on `PATH`

## Python usage

```python
from elbo_sdk.engine import get_engine_communicator

engine = get_engine_communicator(engine_path="/absolute/path/to/pivot_engine")
resp = engine.send_command({"id": 1, "op": "organize_objects"})
```

## Native C++ usage

When building via CMake, `elbo-sdk` provides a C++ library target `elbo_sdk_cpp`.

```cpp
#include <elbo_sdk/engine_client.h>

int main() {
    elbo_sdk::EngineClient client;
    std::string err;

    if (!client.start("/absolute/path/to/pivot_engine", &err)) {
        // handle error
        return 1;
    }

    std::string resp = client.send_command(R"({"id":1,"op":"sync_license"})", &err);
    client.stop();
}
```

## Protocol note

The engine protocol is newline-delimited JSON messages over stdin/stdout.
Large buffers are passed via named shared memory segments.
Engine-side protocol expectations are documented in [engine/app/main.cpp](../engine/app/main.cpp).
