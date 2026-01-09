#include "shm_manager_api.h"

#include "uid.h"

#include <string>

namespace elbo_sdk {

StandardizeSegmentsPlan plan_standardize_segments(std::uint32_t total_verts,
                                                 std::uint32_t total_edges,
                                                 std::uint32_t total_objects) {
    StandardizeSegmentsPlan plan;

    plan.verts_size = static_cast<std::size_t>(total_verts) * 3u * 4u;
    plan.edges_size = static_cast<std::size_t>(total_edges) * 2u * 4u;
    plan.rotations_size = static_cast<std::size_t>(total_objects) * 4u * 4u;
    plan.scales_size = static_cast<std::size_t>(total_objects) * 3u * 4u;
    plan.offsets_size = static_cast<std::size_t>(total_objects) * 3u * 4u;

    plan.uid = new_uid16();

    plan.verts_name = "sp_v_" + plan.uid;
    plan.edges_name = "sp_e_" + plan.uid;
    plan.rotations_name = "sp_r_" + plan.uid;
    plan.scales_name = "sp_s_" + plan.uid;
    plan.offsets_name = "sp_o_" + plan.uid;

    return plan;
}

FaceSizesPlan plan_face_sizes_segment(std::uint32_t total_faces_count) {
    FaceSizesPlan plan;
    plan.face_sizes_size = static_cast<std::size_t>(total_faces_count) * 4u;
    plan.uid = new_uid16();
    plan.face_sizes_name = "sp_fs_" + plan.uid;
    return plan;
}

FacesPlan plan_faces_segment(std::uint32_t total_face_vertices, const std::string& uid) {
    FacesPlan plan;
    plan.faces_size = static_cast<std::size_t>(total_face_vertices) * 4u;
    plan.faces_name = "sp_f_" + uid;
    return plan;
}

} // namespace elbo_sdk
