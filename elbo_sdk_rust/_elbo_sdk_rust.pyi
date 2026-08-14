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


def prepare_mesh_send(
    vert_counts: List[int],
    edge_counts: List[int],
    loop_counts: List[int],
    total_loop_lengths: List[int],
    object_counts: List[int],
    group_names: List[str],
    surface_contexts: List[int],
    asset_uuids: List[bytes],
) -> Tuple["AssetSyncContext", int]: ...


def generate_uuid_bytes() -> bytes: ...


def get_uuid_size() -> int: ...


def get_platform_id() -> str: ...


def set_engine_dir(path: str) -> None: ...


class AssetSyncContext:
    def uuids(self) -> memoryview: ...
    def surface_contexts(self) -> memoryview: ...
    def buffers(self, i: int) -> Tuple[
        memoryview, memoryview, memoryview, memoryview, memoryview,
        memoryview, memoryview, memoryview, memoryview, memoryview, memoryview,
    ]: ...
    def size(self) -> int: ...
    def send(self) -> None: ...


def export_assets_command(path: str, uuids: List[bytes]) -> None: ...


def export_all_command(path: str) -> None: ...


def drop_all_groups_command() -> None: ...


def import_assets_command(paths: List[str]) -> None: ...


def standardize_groups_command(uuids: List[bytes]) -> None: ...


def group_all_objects_command() -> None: ...


class TboImportContext:
    def __init__(self) -> None: ...
    def load_file(self, path: str) -> int: ...
    def unload_file(self, file_idx: int) -> None: ...
    def unload_file_by_path(self, path: str) -> None: ...
    def get_file_info(self, path: str) -> Tuple[int, int, int, int, List[str]]: ...
    def get_hierarchy(self) -> "TBOHierarchy": ...


class TboExportContext:
    """Streaming export context.

    Push geometry batches with prepare_mesh_send() + accumulate(), which also
    drives the shared-memory TBO export. close() performs the final flush and
    returns the list of (format_name, [filenames]) written to disk.
    """

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
    ) -> Tuple[
        memoryview, memoryview, memoryview, memoryview, memoryview,
        memoryview, memoryview, memoryview, memoryview, memoryview, memoryview,
    ]: ...
    def accumulate(self, flush: bool = False) -> List[Tuple[str, List[str]]]: ...
    def close(self) -> List[Tuple[str, List[str]]]: ...
    def get_hierarchy(self) -> "TBOHierarchy":
        """Build a TBOHierarchy over the live export buffers.

        WARNING: The returned hierarchy views shared memory that is overwritten
        in place by the next flush/accumulate/close call on the corresponding
        format's buffer. Use it only for inspection between flushes, or copy
        any data you need to keep. (Import-side hierarchies are safe; they hold
        Arc references to heap-allocated data.)
        """
        ...


class HierarchicalEntity:
    """Cursor over one level of the TBO hierarchy.

    Iteration and indexing yield independent snapshot objects: each element
    is a distinct cursor positioned at that entity, so ``list(frags)`` and
    ``frags[i]`` return separate objects. The cursor you iterate over also
    advances (its ``selected_entity_idx`` reflects the final position after
    iteration), but the yielded snapshots are independent.

    The cursor reports the selected entity's row count (row_count), its
    channel layout (channel_names), and per-entity row data via channel()
    (an (N,) f32 array) and row() (a copied f32 memoryview).

    Child entities can read their parent's selected row by channel name
    (e.g. asset.sx). Unknown names raise AttributeError, as they do for
    top-level entities (which have no parent).

    Example:
        h = ctx.get_hierarchy()
        for scene in h.Scenes:
            for asset in scene.get_child("Assets"):
                print(asset.sx)
                for frag in asset.get_child("Fragments"):
                    for pt in frag.get_child("Points"):
                        x = pt.channel("xyz_x")[0]
    """
    @property
    def row_count(self) -> int: ...
    @property
    def selected_entity_idx(self) -> int:
        """Relative index (0..len-1) of the selected entity within this cursor's range."""
        ...
    @selected_entity_idx.setter
    def selected_entity_idx(self, idx: int) -> None:
        """Set selection by relative index (0..len-1)."""
        ...
    @property
    def channel_names(self) -> List[str]: ...
    def channel(self, name: str) -> np.ndarray: ...
    def row(self, idx: int) -> memoryview:
        """Read-only f32 memoryview of one row. The row data is copied into
        Python-owned bytes, so the view is safe even if the hierarchy is later
        dropped or the underlying buffer is reset."""
        ...
    def get_child(self, name: str) -> Optional["HierarchicalEntity"]: ...
    def __len__(self) -> int: ...
    def __iter__(self) -> Iterator["HierarchicalEntity"]: ...
    def __next__(self) -> "HierarchicalEntity": ...
    def __getitem__(self, idx: int) -> "HierarchicalEntity": ...
    def __getattr__(self, name: str) -> float: ...
    def __repr__(self) -> str: ...


class TBOHierarchy:
    """Linked Scene -> Asset -> Fragment access over loaded TBO data.

    Scene rows align 1:1 with asset entities; asset rows align 1:1 with
    fragment entities. get_hierarchy() rejects file sets that violate this.
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
