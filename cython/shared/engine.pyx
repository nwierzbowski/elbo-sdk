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
- Native process + IPC logic lives in C++ (EngineClient)
- This file only converts Python <-> JSON strings and exposes a small Python API
"""

import json
from typing import Optional

from libcpp.string cimport string

# Direct C bindings to the static C API (delegates to EngineClient internally).
cdef extern from "elbo_sdk/engine_api.h":
    string get_platform_id "elbo_sdk::get_platform_id"() except +
    string resolve_engine_binary_path "elbo_sdk::resolve_engine_binary_path"() except +

    void cpp_start "elbo_sdk::start"(string engine_path) except +
    void cpp_stop "elbo_sdk::stop"() except +
    bint cpp_is_running "elbo_sdk::is_running"() except +

    string send_command "elbo_sdk::send_command"(const string& command_json) except +
    void send_command_async "elbo_sdk::send_command_async"(const string& command_json) except +
    string wait_for_response "elbo_sdk::wait_for_response"(int expected_id) except +

    string sync_license_mode_cpp "elbo_sdk::sync_license_mode_cpp"() except +


def get_platform_id() -> str:
    cdef string s = get_platform_id()
    return (<bytes>s).decode('utf-8', 'replace')


def get_engine_binary_path() -> str:
    """Resolve the pivot_engine binary path.

    Resolution order:
    1) `PIVOT_ENGINE_PATH` env var
    2) `pivot_engine` on `PATH`

    Returns an empty string if no binary can be resolved.
    """
    cdef string s = resolve_engine_binary_path()
    return (<bytes>s).decode('utf-8', 'replace')


def start(engine_path: Optional[str] = None) -> bool:
    cpp_start(engine_path or "")
    return True

def stop() -> None:
    cpp_stop()

def is_running() -> bool:
    return cpp_is_running()

def send_command(dict command_dict) -> dict:
    payload = json.dumps(command_dict)
    resp_line = send_command(payload.encode('utf-8'))

    resp_py = (<bytes>resp_line).decode('utf-8', 'replace')
    return json.loads(resp_py)

def send_command_async(dict command_dict) -> None:
    payload = json.dumps(command_dict)
    send_command_async(payload.encode('utf-8'))

def wait_for_response(int expected_id) -> dict:
    resp_line = wait_for_response(expected_id)

    resp_py = (<bytes>resp_line).decode('utf-8', 'replace')
    return json.loads(resp_py)


def drop_groups(groups) -> int:
    """Drop groups from the engine and return the count of dropped groups."""
    response = send_command({"op": "drop_groups", "groups": groups})
    return response.get("dropped_count", 0)


def sync_license_mode() -> str:
    """Retrieve the compiled edition from the engine."""
    result = sync_license_mode_cpp()
    return (<bytes>result).decode('utf-8', 'replace')

