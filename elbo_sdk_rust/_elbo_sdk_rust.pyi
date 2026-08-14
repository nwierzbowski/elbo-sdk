from typing import List, Dict, Tuple, Optional, Iterator, Any
import numpy as np


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


def get_uuid_size() -> int: ...


def get_platform_id() -> str: ...


def set_engine_dir(path: str) -> None: ...


class AssetSyncContext:
    def uuids(self) -> memoryview: ...
    def surface_contexts(self) -> memoryview: ...
    def buffers(self, i: int) -> Tuple[memoryview, memoryview, memoryview, memoryview, memoryview, memoryview, memoryview, memoryview, memoryview, memoryview]: ...
    def size(self) -> int: ...
    def finalize(self) -> None: ...


def export_assets_command(path: str, uuids: List[bytes]) -> None: ...


def export_all_command(path: str) -> None: ...


def drop_all_groups_command() -> None: ...


def import_assets_command(paths: List[str]) -> None: ...


def standardize_groups_command(uuids: List[bytes]) -> None: ...


def group_all_objects_command() -> None: ...


def tbo_export_command(
    slab_scene_name: bytes,
    slab_asset_name: bytes,
    slab_fragment_name: bytes,
    scene_data_ptr: int,
    scene_offset_ptr: int,
    scene_remaining: int,
    asset_data_ptr: int,
    asset_offset_ptr: int,
    asset_remaining: int,
    frag_data_ptr: int,
    frag_offset_ptr: int,
    frag_remaining: int,
    scene_transform: bool,
    scene_similarity: bool,
    asset_embedding: bool,
    asset_transform: bool,
    fragment_xyz: bool,
    normal_variance: bool,
    surface_variation: bool,
    combined: bool,
    target_point_count: int,
) -> Tuple[int, int, int, int, int, int]: ...


class TboDataBuffer:
    @property
    def path(self) -> str: ...


class TboImportContext:
    def __init__(self) -> None: ...
    def load_file(self, path: str) -> int: ...
    def unload_file(self, file_idx: int) -> None: ...
    def unload_file_by_path(self, path: str) -> None: ...
    def get_file_info(self, path: str) -> Tuple[int, int, int, int, List[str]]: ...
    def get_hierarchy(self) -> "TBOHierarchy": ...


class TboExportContext:
    def __init__(
        self,
        output_dir: str,
        scene_transform: bool,
        scene_similarity: bool,
        asset_embedding: bool,
        asset_transform: bool,
        fragment_xyz: bool,
        normal_variance: bool,
        surface_variation: bool,
        combined: bool,
        max_memory_mb: float,
        target_export_size_mb: float,
        target_point_count: int,
    ) -> None: ...
    def prepare_mesh_send(
        self,
        vert_counts: List[int],
        edge_counts: List[int],
        loop_counts: List[int],
        total_loop_lengths: List[int],
        object_counts: List[int],
        group_names: List[str],
        surface_contexts: List[int],
        asset_uuids: List[bytes],
    ) -> Tuple[Any, ...]: ...
    def accumulate(self, flush: bool = False) -> List[Tuple[str, List[str]]]: ...
    def close(self) -> List[Tuple[str, List[str]]]: ...
    def get_hierarchy(self) -> "TBOHierarchy": ...


# =============================================================================
# HierarchicalEntity - unified navigation proxy
# =============================================================================

class HierarchicalEntity:
    """Unified proxy for navigating the TBO hierarchy.

    Child navigation uses `get_child(name: str) -> Optional[HierarchicalEntity]`.
    Returns None for unknown child names or when no data is available.

    Top-level entities (hierarchy.Scenes/Assets/Fragments) have no parent data;
    __getattr__ returns None. Child entities inherit their parent's row data.

    Example:
        hierarchy = ctx.get_hierarchy()
        for scene in hierarchy.Scenes:
            assets = scene.get_child("Assets")
            if assets is None:
                continue
            for asset in assets:
                print(asset.trans_00, asset.similarity)
                fragments = asset.get_child("Fragments")
                if fragments is None:
                    continue
                for frag in fragments:
                    print(frag.emb_000)
                    for pt in frag.get_child("Points") or []:
                        print(pt.xyz_x, pt.xyz_y, pt.xyz_z)
    """
    @property
    def entity_count(self) -> int: ...
    @property
    def selected_entity_idx(self) -> int: ...
    @selected_entity_idx.setter
    def selected_entity_idx(self, idx: int) -> None: ...
    @property
    def channel_names(self) -> List[str]: ...
    def get_child(self, name: str) -> Optional["HierarchicalEntity"]: ...
    def __len__(self) -> int: ...
    def __iter__(self) -> Iterator["HierarchicalEntity"]: ...
    def __next__(self) -> Optional["HierarchicalEntity"]: ...
    def __getitem__(self, idx: int) -> "HierarchicalEntity": ...
    def __getattr__(self, name: str) -> Optional[float]: ...


# =============================================================================
# Hierarchy - linked three-level access
# =============================================================================

class TBOHierarchy:
    """Unified access to the Scene -> Asset -> Fragment -> Points hierarchy.

    Provides HierarchicalEntity proxies for iteration.
    Child transitions are configurable; new levels can be added without code changes.
    """
    @property
    def Scenes(self) -> HierarchicalEntity: ...
    @property
    def Assets(self) -> HierarchicalEntity: ...
    @property
    def Fragments(self) -> HierarchicalEntity: ...
    @property
    def scene_count(self) -> int: ...
    @property
    def asset_count(self) -> int: ...
    @property
    def fragment_count(self) -> int: ...
