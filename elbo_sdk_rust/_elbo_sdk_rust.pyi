from typing import List, Dict, Tuple, Optional


def start_engine() -> None: ...


def stop_engine() -> None: ...


def standardize_synced_groups_command(
    uuids: List[bytes],
    surface_contexts: List[int],
) -> None: ...


def set_surface_types_command(group_surface_map: Dict[bytes, int]) -> None: ...


def drop_groups_command(uuids: List[bytes]) -> None: ...


def get_surface_types_command() -> None: ...


def organize_objects_command() -> None: ...


def poll_mesh_sync() -> Optional["AssetSyncContext"]: ...


def prepare_standardize_groups(
    vert_counts: List[int],
    edge_counts: List[int],
    loop_counts: List[int],
    total_loop_lengths: List[int],
    object_counts: List[int],
    group_names: List[str],
    surface_contexts: List[int],
    object_uuids: List[bytes],
    asset_uuid: bytes,
) -> "AssetSyncContext": ...


def generate_uuid_bytes() -> bytes: ...


def get_platform_id() -> str: ...


def set_engine_dir(path: str) -> None: ...


class AssetSyncContext:
    def uuids(self) -> memoryview: ...
    def surface_contexts(self) -> memoryview: ...
    def buffers(self, i: int) -> Tuple[memoryview, memoryview, memoryview, memoryview, memoryview, memoryview, memoryview, memoryview, memoryview, memoryview]: ...
    def size(self) -> int: ...
    def finalize(self) -> None: ...
