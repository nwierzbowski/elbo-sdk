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

COMMAND_SET_SURFACE_TYPES = 4
COMMAND_DROP_GROUPS = 5
COMMAND_CLASSIFY_GROUPS = 1
COMMAND_CLASSIFY_OBJECTS = 1
COMMAND_GET_GROUP_SURFACE_TYPES = 2

# Direct C bindings to the static C API (delegates to EngineClient internally).
cdef extern from "engine_api.h":
    string cpp_get_platform_id "elbo_sdk::get_platform_id"() except +
    string cpp_resolve_engine_binary_path "elbo_sdk::resolve_engine_binary_path"() except +

    void cpp_start "elbo_sdk::start"(string engine_path) except +
    void cpp_stop "elbo_sdk::stop"() except +
    bint cpp_is_running "elbo_sdk::is_running"() except +

    string cpp_send_command "elbo_sdk::send_command"(const string& command_json) except +
    void cpp_send_command_async "elbo_sdk::send_command_async"(const string& command_json) except +
    string cpp_wait_for_response "elbo_sdk::wait_for_response"(int expected_id) except +

    string sync_license_mode_cpp "elbo_sdk::sync_license_mode_cpp"() except +


def get_platform_id() -> str:
    print("Getting platform ID")
    cdef string s = cpp_get_platform_id()
    print("Platform ID:", (<bytes>s).decode('utf-8', 'replace'))
    return (<bytes>s).decode('utf-8', 'replace')


def get_engine_binary_path() -> str:
    """Resolve the pivot_engine binary path.

    Resolution order:
    1) `PIVOT_ENGINE_PATH` env var
    2) `pivot_engine` on `PATH`

    Returns an empty string if no binary can be resolved.
    """
    cdef string s = cpp_resolve_engine_binary_path()
    return (<bytes>s).decode('utf-8', 'replace')


def start(engine_path: Optional[str] = None) -> bool:
    cpp_start(engine_path.encode('utf-8') or "")
    return True

def stop() -> None:
    cpp_stop()

def is_running() -> bool:
    return cpp_is_running()

def send_command(dict command_dict) -> dict:
    print("Sending command:", command_dict)
    payload = json.dumps(command_dict)
    resp_line = cpp_send_command(payload.encode('utf-8'))

    resp_py = (<bytes>resp_line).decode('utf-8', 'replace')
    return json.loads(resp_py)

def send_command_async(dict command_dict) -> None:
    print("Sending async command:", command_dict)
    payload = json.dumps(command_dict)
    cpp_send_command_async(payload.encode('utf-8'))

def wait_for_response(int expected_id) -> dict:
    resp_line = cpp_wait_for_response(expected_id)

    resp_py = (<bytes>resp_line).decode('utf-8', 'replace')
    return json.loads(resp_py)


def drop_groups(groups) -> int:
    """Drop groups from the engine and return the count of dropped groups."""
    response = send_command({"op": "drop_groups", "groups": groups})
    return response.get("dropped_count", 0)


def sync_license_mode() -> str:
    """Retrieve the compiled edition from the engine."""
    print("Syncing license mode with engine...")
    result = sync_license_mode_cpp()

    s = (<bytes>result).decode('utf-8', 'replace')
    try:
        data = json.loads(s)
        return data.get("engine_edition", "unknown")
    except Exception:
        return s

def build_standardize_groups_command(verts_shm_name: str, edges_shm_name: str, 
                                    rotations_shm_name: str, scales_shm_name: str, 
                                    offsets_shm_name: str, vert_counts: list, 
                                    edge_counts: list, object_counts: list, 
                                    group_names: list, surface_contexts: list[str]) -> Dict[str, Any]:
    """Build a standardize_groups command for the engine (Pro edition).
    
    Args:
        verts_shm_name: Shared memory name for vertex data
        edges_shm_name: Shared memory name for edge data
        rotations_shm_name: Shared memory name for rotation data
        scales_shm_name: Shared memory name for scale data
        offsets_shm_name: Shared memory name for offset data
        vert_counts: List of vertex counts per group
        edge_counts: List of edge counts per group
        object_counts: List of object counts per group
        group_names: List of group names to standardize
        surface_context: Surface context for standardization
        
    Returns:
        Dict containing the command structure
    """
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

def build_standardize_synced_groups_command(group_names: list[str], surface_contexts: list[str]) -> Dict[str, Any]:
    """Build a command to reclassify already-synced groups without uploading mesh data."""
    return {
        "id": COMMAND_CLASSIFY_GROUPS,
        "op": "standardize_synced_groups",
        "group_names": group_names,
        "surface_contexts": surface_contexts
    }

def build_standardize_objects_command(verts_shm_name: str, edges_shm_name: str,
                                    rotations_shm_name: str, scales_shm_name: str,
                                    offsets_shm_name: str, vert_counts: list,
                                    edge_counts: list, object_names: list, surface_contexts: list[str]) -> Dict[str, Any]:
    """Build a standardize_objects command for the engine.
    
    Args:
        verts_shm_name: Shared memory name for vertex data
        edges_shm_name: Shared memory name for edge data
        rotations_shm_name: Shared memory name for rotation data
        scales_shm_name: Shared memory name for scale data
        offsets_shm_name: Shared memory name for offset data
        vert_counts: List of vertex counts per object
        edge_counts: List of edge counts per object
        object_names: List of object names to standardize
        surface_contexts: Per-object surface context strings
        
    Returns:
        Dict containing the command structure
    """
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

def send_group_classifications(group_surface_map: Dict[str, Any]) -> bool:
    """Send a batch classification update to the engine."""
    if not group_surface_map:
        return True

    if not is_running():
        return False

    payload = []
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
        response = send_command(command)
        if not response.get("ok", False):
            error = response.get("error", "Unknown error")
            print(f"Failed to update group classifications: {error}")
            return False
        return True
    except Exception as exc:
        print(f"Error sending group classifications: {exc}")
        return False

def drop_groups(group_names: list[str]) -> int:
    """Drop groups from the engine cache.

    Args:
        group_names: List of group names to drop from the cache

    Returns:
        int: Number of groups actually dropped, or -1 on error
    """
    if not group_names:
        return 0

    if not is_running():
        return -1

    try:
        command = {
            "id": COMMAND_DROP_GROUPS,
            "op": "drop_groups",
            "group_names": group_names
        }
        response = send_command(command)
        if not response.get("ok", False):
            error = response.get("error", "Unknown error")
            print(f"Failed to drop groups from engine: {error}")
            return -1
        dropped_count = response.get("dropped_count", 0)
        return dropped_count
    except Exception as exc:
        print(f"Error dropping groups from engine: {exc}")
        return -1    

def build_get_surface_types_command() -> Dict[str, Any]:
    """Build a get_surface_types command for the engine.
    
    Returns:
        Dict containing the command structure
    """
    return {
        "id": COMMAND_GET_GROUP_SURFACE_TYPES,
        "op": "get_surface_types"
    }