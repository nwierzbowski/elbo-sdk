# distutils: language = c++
"""Shared-memory allocation helpers (bridge-agnostic).

This module owns the lifecycle of shared memory segments used for transferring
large buffers into the engine. Other bridges (Blender, etc.) should request
segments from here, then write directly into the exposed raw buffers.

It intentionally contains no `bpy`/host-API logic.
"""

import uuid
from libc.stdint cimport uint32_t
from libc.stddef cimport size_t

from elbo_sdk.shm_bridge import SharedMemory


def _new_uid() -> str:
    # Keep names short (macOS POSIX shm name length constraints).
    return uuid.uuid4().hex[:16]


def create_standardize_segments(uint32_t total_verts, uint32_t total_edges, uint32_t total_objects):
    """Create shared memory segments for standardize ops.

    Returns:
        (shm_objects, shm_names)

    shm_objects: (verts_shm, edges_shm, rotations_shm, scales_shm, offsets_shm)
    shm_names:   (verts_name, edges_name, rotations_name, scales_name, offsets_name)
    """
    cdef size_t verts_size = <size_t>total_verts * 3 * 4
    cdef size_t edges_size = <size_t>total_edges * 2 * 4
    cdef size_t rotations_size = <size_t>total_objects * 4 * 4
    cdef size_t scales_size = <size_t>total_objects * 3 * 4
    cdef size_t offsets_size = <size_t>total_objects * 3 * 4

    uid = _new_uid()

    verts_name = f"sp_v_{uid}"
    edges_name = f"sp_e_{uid}"
    rotations_name = f"sp_r_{uid}"
    scales_name = f"sp_s_{uid}"
    offsets_name = f"sp_o_{uid}"

    verts_shm = SharedMemory(create=True, size=verts_size, name=verts_name)
    edges_shm = SharedMemory(create=True, size=edges_size, name=edges_name)
    rotations_shm = SharedMemory(create=True, size=rotations_size, name=rotations_name)
    scales_shm = SharedMemory(create=True, size=scales_size, name=scales_name)
    offsets_shm = SharedMemory(create=True, size=offsets_size, name=offsets_name)

    return (verts_shm, edges_shm, rotations_shm, scales_shm, offsets_shm), (verts_name, edges_name, rotations_name, scales_name, offsets_name)


def create_face_sizes_segment(uint32_t total_faces_count):
    """Create the face-sizes segment.

    Returns:
        (face_sizes_shm, face_sizes_name, uid)
    """
    cdef size_t face_sizes_size = <size_t>total_faces_count * 4
    uid = _new_uid()
    face_sizes_name = f"sp_fs_{uid}"
    face_sizes_shm = SharedMemory(create=True, size=face_sizes_size, name=face_sizes_name)
    return face_sizes_shm, face_sizes_name, uid


def create_faces_segment(uint32_t total_face_vertices, str uid):
    """Create the faces-index segment using the same uid as the sizes segment."""
    cdef size_t faces_size = <size_t>total_face_vertices * 4
    faces_name = f"sp_f_{uid}"
    faces_shm = SharedMemory(create=True, size=faces_size, name=faces_name)
    return faces_shm, faces_name
