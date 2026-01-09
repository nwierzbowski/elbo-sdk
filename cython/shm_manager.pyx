# distutils: language = c++
"""Shared-memory allocation helpers (bridge-agnostic).

The Cython layer is intentionally thin:
- Shared-memory name/size planning lives in C++ (see `shm_manager_api.h`)
- This file only instantiates `SharedMemory` objects and returns Python tuples

It intentionally contains no `bpy`/host-API logic.
"""

from libc.stdint cimport uint32_t
from libcpp.string cimport string

from .shm_bridge import SharedMemory


def create_standardize_segments(uint32_t total_verts, uint32_t total_edges, uint32_t total_objects):
    """Create shared memory segments for standardize ops.

    Returns:
        (shm_buffers, shm_names)

    shm_buffers: (verts_buf, edges_buf, rotations_buf, scales_buf, offsets_buf)
    shm_names:   (verts_name, edges_name, rotations_name, scales_name, offsets_name)
    
    Note: Keep a reference to the returned buffers tuple to prevent garbage collection
    of the underlying shared memory segments.
    """
    cdef StandardizeSegmentsPlan plan = plan_standardize_segments(total_verts, total_edges, total_objects)

    verts_name = (<bytes>plan.verts_name).decode('utf-8', 'replace')
    edges_name = (<bytes>plan.edges_name).decode('utf-8', 'replace')
    rotations_name = (<bytes>plan.rotations_name).decode('utf-8', 'replace')
    scales_name = (<bytes>plan.scales_name).decode('utf-8', 'replace')
    offsets_name = (<bytes>plan.offsets_name).decode('utf-8', 'replace')

    verts_shm = SharedMemory(create=True, size=plan.verts_size, name=verts_name)
    edges_shm = SharedMemory(create=True, size=plan.edges_size, name=edges_name)
    rotations_shm = SharedMemory(create=True, size=plan.rotations_size, name=rotations_name)
    scales_shm = SharedMemory(create=True, size=plan.scales_size, name=scales_name)
    offsets_shm = SharedMemory(create=True, size=plan.offsets_size, name=offsets_name)

    # Return memoryview buffers directly, not SharedMemory objects
    verts_buf = verts_shm.buf
    edges_buf = edges_shm.buf
    rotations_buf = rotations_shm.buf
    scales_buf = scales_shm.buf
    offsets_buf = offsets_shm.buf

    return (verts_buf, edges_buf, rotations_buf, scales_buf, offsets_buf), (verts_name, edges_name, rotations_name, scales_name, offsets_name)


def create_face_sizes_segment(uint32_t total_faces_count):
    """Create the face-sizes segment.

    Returns:
        (face_sizes_buf, face_sizes_name, uid)
        
    Note: Keep a reference to the returned buffer to prevent garbage collection
    of the underlying shared memory segment.
    """
    cdef FaceSizesPlan plan = plan_face_sizes_segment(total_faces_count)
    uid = (<bytes>plan.uid).decode('utf-8', 'replace')
    face_sizes_name = (<bytes>plan.face_sizes_name).decode('utf-8', 'replace')
    face_sizes_shm = SharedMemory(create=True, size=plan.face_sizes_size, name=face_sizes_name)
    face_sizes_buf = face_sizes_shm.buf
    return face_sizes_buf, face_sizes_name, uid


def create_faces_segment(uint32_t total_face_vertices, str uid):
    """Create the faces-index segment using the same uid as the sizes segment.
    
    Returns:
        (faces_buf, faces_name)
        
    Note: Keep a reference to the returned buffer to prevent garbage collection
    of the underlying shared memory segment.
    """
    cdef string uid_s = uid.encode('utf-8')
    cdef FacesPlan plan = plan_faces_segment(total_face_vertices, uid_s)
    faces_name = (<bytes>plan.faces_name).decode('utf-8', 'replace')
    faces_shm = SharedMemory(create=True, size=plan.faces_size, name=faces_name)
    faces_buf = faces_shm.buf
    return faces_buf, faces_name
