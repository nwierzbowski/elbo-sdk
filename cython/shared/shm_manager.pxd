from libcpp.string cimport string
from libc.stdint cimport uint32_t
from libc.stddef cimport size_t

cdef extern from "elbo_sdk/shm_manager_api.h" namespace "elbo_sdk":
    cdef cppclass StandardizeSegmentsPlan:
        string uid
        string verts_name
        string edges_name
        string rotations_name
        string scales_name
        string offsets_name
        size_t verts_size
        size_t edges_size
        size_t rotations_size
        size_t scales_size
        size_t offsets_size

    StandardizeSegmentsPlan plan_standardize_segments(uint32_t total_verts, uint32_t total_edges, uint32_t total_objects) except +

    cdef cppclass FaceSizesPlan:
        string uid
        string face_sizes_name
        size_t face_sizes_size

    FaceSizesPlan plan_face_sizes_segment(uint32_t total_faces_count) except +

    cdef cppclass FacesPlan:
        string faces_name
        size_t faces_size

    FacesPlan plan_faces_segment(uint32_t total_face_vertices, const string& uid) except +
