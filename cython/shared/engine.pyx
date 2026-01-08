# Copyright (C) 2025 [Nicholas Wierzbowski/Elbo Studio]

# This file is part of the Pivot Bridge for Blender.

# The Pivot Bridge for Blender is free software; you can redistribute it and/or
# modify it under the terms of the GNU General Public License
# as published by the Free Software Foundation; either version 3
# of the License, or (at your option) any later version.

# This program is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
# GNU General Public License for more details.

# You should have received a copy of the GNU General Public License
# along with this program; if not, see <https://www.gnu.org/licenses>.

"""Pivot Engine IPC client (SDK).

This module intentionally contains no Blender (`bpy`) interactions.

High-level responsibilities:
- Launch/stop the engine process (native C++ client)
- Send JSON commands over stdin/stdout and read responses
- Build engine command payloads (Python dict helpers)

Engine binary discovery is host-specific. For portability, the SDK supports:
- Explicit `engine_path=` arguments
- `PIVOT_ENGINE_PATH` environment variable
"""

import os
import atexit
import json
import platform
import builtins
import shutil
from typing import Optional

from libcpp.string cimport string

cdef extern from "elbo_sdk/engine_client.h" namespace "elbo_sdk":
    cdef cppclass EngineClient:
        EngineClient() except +
        bint start(const string& engine_path, string* error_out) except +
        void stop() except +
        bint is_running() const
        string send_command(const string& command_json, string* error_out) except +
        void send_command_async(const string& command_json, string* error_out) except +
        string wait_for_response(int expected_id, string* error_out) except +

# Command IDs for engine communication
cdef int COMMAND_SET_SURFACE_TYPES = 4
cdef int COMMAND_DROP_GROUPS = 5
cdef int COMMAND_CLASSIFY_GROUPS = 1
cdef int COMMAND_CLASSIFY_OBJECTS = 1
cdef int COMMAND_GET_GROUP_SURFACE_TYPES = 2


def get_engine_binary_path() -> str:
    """Resolve the pivot_engine binary path.

    For portability, this function does not inspect Blender modules or repo layouts.
    Host applications should prefer passing `engine_path=` to `get_engine_communicator()`.

    Resolution order:
    1) `PIVOT_ENGINE_PATH` env var
    2) `pivot_engine` on PATH
    3) `bin/<platform_id>/pivot_engine` relative to this package (optional)
    """
    env_path = os.getenv("PIVOT_ENGINE_PATH")
    if env_path:
        return env_path

    exe_name = "pivot_engine.exe" if platform.system().lower() == "windows" else "pivot_engine"
    which = shutil.which(exe_name)
    if which:
        return which

    module_dir = os.path.dirname(__file__) if '__file__' in dir() else os.getcwd()
    bin_dir = os.path.join(module_dir, "bin")
    platform_dir = os.path.join(bin_dir, get_platform_id())
    platform_binary = os.path.join(platform_dir, exe_name)
    if os.path.exists(platform_binary):
        return platform_binary

    fallback_binary = os.path.join(bin_dir, exe_name)
    return fallback_binary


def get_platform_id() -> str:
    """Get platform identifier for module loading (e.g., 'linux-x86-64', 'macos-arm64').
    
    Returns:
        str: Platform identifier string
    """
    system = platform.system().lower()
    machine = platform.machine().lower()
    
    # Map architecture names
    if machine in ('x86_64', 'amd64'):
        arch = 'x86-64'
    elif machine in ('aarch64', 'arm64'):
        arch = 'arm64'
    else:
        arch = machine
    
    return f'{system}-{arch}'


cdef class PivotEngine:
    """Unified interface for the C++ pivot engine subprocess."""

    cdef EngineClient* _client

    def __init__(self):
        # Kept for backward compatibility; actual init is in __cinit__.
        pass

    def __cinit__(self):
        self._client = new EngineClient()

    def __dealloc__(self):
        try:
            if self._client is not NULL:
                self._client.stop()
        except Exception:
            pass
        if self._client is not NULL:
            del self._client
            self._client = NULL

    def start(self, engine_path: Optional[str] = None) -> bint:
        """Start the pivot engine executable.

        Returns:
            bool: True if started successfully, False otherwise
        """
        cdef string err

        try:
            if self.is_running():
                return True

            resolved = engine_path or get_engine_binary_path()

            if not resolved or not os.path.exists(resolved):
                print(f"Warning: Engine executable not found at {resolved}")
                return False

            ok = self._client.start(resolved.encode('utf-8'), &err)
            if not ok:
                err_py = (<bytes>err).decode('utf-8', 'replace') if err.size() else "unknown error"
                print(f"Failed to start pivot engine: {err_py}")
                return False

            return True
        except Exception as e:
            print(f"Failed to start pivot engine: {e}")
            return False

    def stop(self) -> None:
        """Stop the pivot engine executable."""
        if self._client is NULL:
            return
        self._client.stop()

    def is_running(self) -> bint:
        """Check if the engine is currently running."""
        if self._client is NULL:
            return False
        return self._client.is_running()

    def send_command(self, dict command_dict) -> dict:
        """Send a command to the engine and get the final response."""
        cdef string err

        if not self.is_running():
            raise RuntimeError("Engine process not started or has terminated.")

        payload = json.dumps(command_dict)
        resp_line = self._client.send_command(payload.encode('utf-8'), &err)
        if resp_line.size() == 0:
            err_py = (<bytes>err).decode('utf-8', 'replace') if err.size() else "unknown error"
            raise RuntimeError(f"Communication error: {err_py}")

        resp_py = (<bytes>resp_line).decode('utf-8', 'replace')
        return json.loads(resp_py)

    def send_command_async(self, dict command_dict) -> None:
        """Send a command to the engine without waiting for response."""
        cdef string err

        if not self.is_running():
            raise RuntimeError("Engine process not started or has terminated.")

        payload = json.dumps(command_dict)
        self._client.send_command_async(payload.encode('utf-8'), &err)
        if err.size():
            err_py = (<bytes>err).decode('utf-8', 'replace')
            raise RuntimeError(f"Communication error: {err_py}")

    def wait_for_response(self, int expected_id) -> dict:
        """Wait for a response with the specified ID."""
        cdef string err

        if not self.is_running():
            raise RuntimeError("Engine process not started or has terminated.")

        resp_line = self._client.wait_for_response(expected_id, &err)
        if resp_line.size() == 0:
            err_py = (<bytes>err).decode('utf-8', 'replace') if err.size() else "unknown error"
            raise RuntimeError(f"Communication error: {err_py}")

        resp_py = (<bytes>resp_line).decode('utf-8', 'replace')
        return json.loads(resp_py)

    def send_group_classifications(self, dict group_surface_map) -> bint:
        """Send a batch classification update to the engine."""
        if not group_surface_map:
            return True

        if not self.is_running():
            return False

        cdef list payload = []
        for name, value in group_surface_map.items():
            try:
                surface_int = int(value)
            except (TypeError, ValueError):
                continue
            payload.append({"group_name": name, "surface_type": surface_int})

        if not payload:
            return True

        try:
            command = {
                "id": COMMAND_SET_SURFACE_TYPES,
                "op": "set_surface_types",
                "classifications": payload
            }
            response = self.send_command(command)
            if not response.get("ok", False):
                error = response.get("error", "Unknown error")
                print(f"Failed to update group classifications: {error}")
                return False
            return True
        except Exception as exc:
            print(f"Error sending group classifications: {exc}")
            return False

    def drop_groups(self, list group_names) -> int:
        """Drop groups from the engine cache."""
        if not group_names:
            return 0

        if not self.is_running():
            return -1

        try:
            command = {
                "id": COMMAND_DROP_GROUPS,
                "op": "drop_groups",
                "group_names": group_names
            }
            response = self.send_command(command)
            if not response.get("ok", False):
                error = response.get("error", "Unknown error")
                print(f"Failed to drop groups from engine: {error}")
                return -1
            dropped_count = response.get("dropped_count", 0)
            return dropped_count
        except Exception as exc:
            print(f"Error dropping groups from engine: {exc}")
            return -1

    def build_standardize_groups_command(self, str verts_shm_name, str edges_shm_name, 
                                     str rotations_shm_name, str scales_shm_name, 
                                     str offsets_shm_name, list vert_counts, 
                                     list edge_counts, list object_counts, 
                                     list group_names, list surface_contexts) -> dict:
        """Build a standardize_groups command for the engine (Pro edition)."""
        return {
            "id": COMMAND_CLASSIFY_GROUPS,
            "op": "standardize_groups",
            "shm_verts": verts_shm_name,
            "shm_edges": edges_shm_name,
            "shm_rotations": rotations_shm_name,
            "shm_scales": scales_shm_name,
            "shm_offsets": offsets_shm_name,
            "vert_counts": vert_counts,
            "edge_counts": edge_counts,
            "object_counts": object_counts,
            "group_names": group_names,
            "surface_contexts": surface_contexts,
        }

    def build_standardize_synced_groups_command(self, list group_names, list surface_contexts) -> dict:
        """Build a command to reclassify already-synced groups without uploading mesh data."""
        return {
            "id": COMMAND_CLASSIFY_GROUPS,
            "op": "standardize_synced_groups",
            "group_names": group_names,
            "surface_contexts": surface_contexts
        }

    def build_standardize_objects_command(self, str verts_shm_name, str edges_shm_name,
                                      str rotations_shm_name, str scales_shm_name,
                                      str offsets_shm_name, list vert_counts,
                                      list edge_counts, list object_names, list surface_contexts) -> dict:
        """Build a standardize_objects command for the engine."""
        return {
            "id": COMMAND_CLASSIFY_OBJECTS,
            "op": "standardize_objects",
            "shm_verts": verts_shm_name,
            "shm_edges": edges_shm_name,
            "shm_rotations": rotations_shm_name,
            "shm_scales": scales_shm_name,
            "shm_offsets": offsets_shm_name,
            "vert_counts": vert_counts,
            "edge_counts": edge_counts,
            "object_names": object_names,
            "surface_contexts": surface_contexts
        }

    def build_get_surface_types_command(self) -> dict:
        """Build a get_surface_types command for the engine."""
        return {
            "id": COMMAND_GET_GROUP_SURFACE_TYPES,
            "op": "get_surface_types"
        }


# Global engine instance stored on builtins to persist across reloads
cdef PivotEngine _engine_instance

_temp_instance = getattr(builtins, '_pivot_engine_instance', None)
if _temp_instance is None:
    _engine_instance = PivotEngine()
    builtins._pivot_engine_instance = _engine_instance
else:
    _engine_instance = _temp_instance
    if _engine_instance.is_running():
        _engine_instance.stop()


def start_engine() -> bint:
    """Start the pivot engine (convenience function)."""
    return _engine_instance.start()


def stop_engine() -> None:
    """Stop the pivot engine (convenience function)."""
    _engine_instance.stop()


def get_engine_communicator(engine_path: Optional[str] = None) -> PivotEngine:
    """Get the engine instance for communication."""
    if not _engine_instance.is_running():
        started = _engine_instance.start(engine_path=engine_path)
        if not started:
            raise RuntimeError("Engine process not started. Make sure the addon is properly registered.")
    return _engine_instance


def get_engine_process():
    """Get the current engine process (for backward compatibility)."""
    # The SDK no longer exposes a raw subprocess handle.
    return None


def sync_license_mode() -> str:
    """Retrieve the compiled edition from the engine."""
    engine_comm = get_engine_communicator()
    payload = {
        "id": 0,
        "op": "sync_license",
    }
    response = engine_comm.send_command(payload)
    engine_mode = str(response.get("engine_edition", "UNKNOWN")).upper()
    return engine_mode


# Register cleanup function to run on Python exit
atexit.register(stop_engine)
