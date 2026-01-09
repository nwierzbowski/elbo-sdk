#pragma once

#include <cstddef>
#include <cstdint>
#include <string>

namespace elbo_sdk {

struct StandardizeSegmentsPlan {
    std::string uid;

    std::string verts_name;
    std::string edges_name;
    std::string rotations_name;
    std::string scales_name;
    std::string offsets_name;

    std::size_t verts_size = 0;
    std::size_t edges_size = 0;
    std::size_t rotations_size = 0;
    std::size_t scales_size = 0;
    std::size_t offsets_size = 0;
};

StandardizeSegmentsPlan plan_standardize_segments(std::uint32_t total_verts,
                                                 std::uint32_t total_edges,
                                                 std::uint32_t total_objects);

struct FaceSizesPlan {
    std::string uid;
    std::string face_sizes_name;
    std::size_t face_sizes_size = 0;
};

FaceSizesPlan plan_face_sizes_segment(std::uint32_t total_faces_count);

struct FacesPlan {
    std::string faces_name;
    std::size_t faces_size = 0;
};

FacesPlan plan_faces_segment(std::uint32_t total_face_vertices, const std::string& uid);

} // namespace elbo_sdk
