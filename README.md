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

## Shared memory semantics (client vs engine)

- Client SDK responsibilities:
    - Plan shared memory segment names and sizes using the `shm_manager` API (e.g. `plan_standardize_segments`, `plan_face_sizes_segment`, `plan_faces_segment`).
    - Create mappings and expose zero-copy `memoryview` buffers to Python via the Cython wrappers (e.g. `shm_manager.create_standardize_segments(...)` returns buffers and names).
    - The client process must unmap its own mappings when the `SharedMemory` wrapper is garbage-collected (this is handled automatically via RAII in the SDK; do not call any unlink/remove functions from client code).

- Engine responsibilities:
    - The engine process is the authoritative owner of shared-memory segment lifecycle and must `unlink` (remove) named segments when the data is globally finished. The client SDK intentionally does not expose unlink/removal APIs.

## Naming and UID conventions

- Standardize segments use a generated UID and follow the naming pattern: `sp_v_<uid>`, `sp_e_<uid>`, `sp_r_<uid>`, `sp_s_<uid>`, `sp_o_<uid>`.
- Face segments are created in two phases: a `face_sizes` segment is created first (producing a UID and the `sp_fs_<uid>` name), then the `faces` segment is created using the same UID with name `sp_f_<uid>`.

## Examples

Python (high-level, zero-copy buffers):

```python
from elbo_sdk import shm_manager

# Plan + create standardize segments
(verts_buf, edges_buf, rotations_buf, scales_buf, offsets_buf), names = \
        shm_manager.create_standardize_segments(total_verts, total_edges, total_objects)

# `*_buf` are `memoryview` objects you can wrap with NumPy without copying
import numpy as np
verts = np.ndarray((total_verts * 3,), dtype=np.float32, buffer=verts_buf)

# Keep `verts_buf` (or the returned tuple) alive while views are used to avoid premature unmapping
```

C++ (planning API):

```cpp
#include <elbo_sdk/shm_manager_api.h>

auto plan = elbo_sdk::plan_standardize_segments(total_verts, total_edges, total_objects);
// plan contains generated names and sizes; use these names to coordinate with the engine
```

## Notes & Best Practices

- Do not rely on client-side unlinking; the engine will remove segments when appropriate.
- Always keep a reference to returned buffers while they are in use to prevent RAII cleanup from unmapping the memory prematurely.
- If you need the engine to persist segments beyond the client process lifetime, coordinate via the engine IPC (engine should create and manage the named segments).
