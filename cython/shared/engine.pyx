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

The Cython layer is intentionally thin:
- Native process + IPC logic lives in C++ (`PivotEngineApi`)
- This file only converts Python <-> JSON strings and exposes a small Python API
"""

import atexit
import builtins
import json
from typing import Optional

from libcpp.string cimport string

cimport engine as engine_cpp


def get_platform_id() -> str:
    cdef string s = engine_cpp._cpp_get_platform_id()
    return (<bytes>s).decode('utf-8', 'replace')


def get_engine_binary_path() -> str:
    """Resolve the pivot_engine binary path.

    Resolution order:
    1) `PIVOT_ENGINE_PATH` env var
    2) `pivot_engine` on `PATH`

    Returns an empty string if no binary can be resolved.
    """
    cdef string s = engine_cpp.resolve_engine_binary_path()
    return (<bytes>s).decode('utf-8', 'replace')


cdef class PivotEngine:
    cdef engine_cpp.PivotEngineApi* _api

    def __cinit__(self):
        # The singleton lives in C++ and persists across module reloads.
        self._api = &engine_cpp.engine_singleton()

    def start(self, engine_path: Optional[str] = None):
        cdef string err
        cdef string path

        if engine_path is None:
            engine_path = get_engine_binary_path()

        if engine_path is None:
            engine_path = ""

        path = engine_path.encode('utf-8')
        ok = self._api.start(path, &err)
        if not ok:
            err_py = (<bytes>err).decode('utf-8', 'replace') if err.size() else "unknown error"
            print(f"Failed to start pivot engine: {err_py}")
        return ok

    def stop(self) -> None:
        self._api.stop()

    def is_running(self):
        return self._api.is_running()

    def send_command(self, dict command_dict) -> dict:
        cdef string err
        cdef string resp_line

        if not self.is_running():
            raise RuntimeError("Engine process not started or has terminated.")

        payload = json.dumps(command_dict)
        resp_line = self._api.send_command(payload.encode('utf-8'), &err)
        if resp_line.size() == 0:
            err_py = (<bytes>err).decode('utf-8', 'replace') if err.size() else "unknown error"
            raise RuntimeError(f"Communication error: {err_py}")

        resp_py = (<bytes>resp_line).decode('utf-8', 'replace')
        return json.loads(resp_py)

    def send_command_async(self, dict command_dict) -> None:
        cdef string err

        if not self.is_running():
            raise RuntimeError("Engine process not started or has terminated.")

        payload = json.dumps(command_dict)
        self._api.send_command_async(payload.encode('utf-8'), &err)
        if err.size():
            err_py = (<bytes>err).decode('utf-8', 'replace')
            raise RuntimeError(f"Communication error: {err_py}")

    def wait_for_response(self, int expected_id) -> dict:
        cdef string err
        cdef string resp_line

        if not self.is_running():
            raise RuntimeError("Engine process not started or has terminated.")

        resp_line = self._api.wait_for_response(expected_id, &err)
        if resp_line.size() == 0:
            err_py = (<bytes>err).decode('utf-8', 'replace') if err.size() else "unknown error"
            raise RuntimeError(f"Communication error: {err_py}")

        resp_py = (<bytes>resp_line).decode('utf-8', 'replace')
        return json.loads(resp_py)


# Global engine instance stored on builtins to persist across reloads
_temp_instance = getattr(builtins, '_pivot_engine_instance', None)
if _temp_instance is None:
    _engine_instance = PivotEngine()
    builtins._pivot_engine_instance = _engine_instance
else:
    _engine_instance = _temp_instance
    try:
        if _engine_instance.is_running():
            _engine_instance.stop()
    except Exception:
        pass


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


atexit.register(stop_engine)
