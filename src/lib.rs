mod asset_sync_context;
mod command_thread;
mod engine_api;
mod engine_client; // This line remains unchanged
mod mesh_sync_thread;
mod tbo;
extern crate iceoryx2_loggers;

use pyo3::prelude::*;

#[pymodule(name = "_elbo_sdk_rust")]
mod elbo_sdk_rust {
    use crate::asset_sync_context::AssetSyncContext;
    use crate::engine_api;
    use crate::tbo::{TboExportContext, TBOHierarchy, TboImportContext, HierarchicalEntity};
    use pivot_com_types::fields::Uuid;
    use pyo3::prelude::*;
    use std::path::PathBuf;

    #[pyfunction]
    fn start_engine(_py: Python) -> PyResult<()> {
        engine_api::start_engine()
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))
    }

    #[pyfunction]
    fn stop_engine(_py: Python) -> PyResult<()> {
        engine_api::stop_engine()
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))
    }

    #[pyfunction]
    fn standardize_synced_groups_command(
        _py: Python,
        uuids: Vec<Uuid>,
        surface_contexts: Vec<u32>,
    ) -> () {
        let _ = engine_api::standardize_synced_groups_command(uuids, surface_contexts)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()));
    }

    #[pyfunction]
    fn set_surface_types_command(
        _py: Python,
        group_surface_map: std::collections::HashMap<Uuid, i64>,
    ) -> () {
        let _ = engine_api::set_surface_types_command(group_surface_map)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()));
    }

    #[pyfunction]
    fn drop_groups_command(_py: Python, uuids: Vec<Uuid>) -> () {
        let _ = engine_api::drop_groups_command(uuids)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()));
    }

    #[pyfunction]
    fn get_surface_types_command(_py: Python) -> () {
        let _ = engine_api::get_surface_types_command()
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()));
    }

    #[pyfunction]
    fn organize_objects_command(_py: Python) -> () {
        let _ = engine_api::organize_objects_command()
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()));
    }

    #[pyfunction]
    fn get_platform_id() -> PyResult<String> {
        Ok(crate::engine_api::get_platform_id())
    }

    #[pyfunction]
    fn set_engine_dir(path: String) -> PyResult<()> {
        crate::engine_api::set_engine_dir(PathBuf::from(path));
        Ok(())
    }

    #[pyfunction]
    fn poll_mesh_sync() -> PyResult<Option<AssetSyncContext>> {
        let context = match engine_api::poll_mesh_sync() {
            Ok(Some(slices)) => slices,
            Ok(None) => return Ok(None),
            Err(e) => {
                return Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                    e.to_string(),
                ));
            }
        };

        Ok(Some(context))
    }

    #[pyfunction]
    fn prepare_mesh_send(
        vert_counts: Vec<u32>,
        edge_counts: Vec<u32>,
        loop_counts: Vec<u32>,
        total_loop_lengths: Vec<u32>,
        object_counts: Vec<u32>,
        group_names: Vec<String>,
        surface_contexts: Vec<u16>,
        asset_uuids: Vec<Uuid>,
    ) -> PyResult<(AssetSyncContext, u64)> {
        let (context, allocated_bytes) = engine_api::allocate_memory(
            vert_counts,
            edge_counts,
            loop_counts,
            total_loop_lengths,
            object_counts,
            group_names,
            surface_contexts,
            asset_uuids,
        )
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;

        Ok((context, allocated_bytes))
    }

    #[pyfunction]
    fn standardize_groups_command(_py: Python, uuids: Vec<Uuid>) -> () {
        let _ = engine_api::standardize_groups_command(uuids)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()));
    }

    #[pyfunction]
    fn generate_uuid_bytes() -> PyResult<Vec<u8>> {
        Ok(engine_api::generate_uuid_bytes().to_vec())
    }

    #[pyfunction]
    fn get_uuid_size() -> usize {
        engine_api::get_uuid_size()
    }

    #[pyfunction]
    fn export_assets_command(
        _py: Python,
        path: String,
        uuids: Vec<Uuid>,
    ) -> () {
        let _ = engine_api::export_assets_command(&path, uuids)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()));
    }

    #[pyfunction]
    fn export_all_command(_py: Python, path: String) -> () {
        let _ = engine_api::export_all_command(&path)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()));
    }

    #[pyfunction]
    fn drop_all_groups_command(_py: Python) -> () {
        let _ = engine_api::drop_all_groups_command()
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()));
    }

    #[pyfunction]
    fn import_assets_command(_py: Python, paths: Vec<String>) -> () {
        let _ = engine_api::import_assets_command(paths)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()));
    }

    #[pymodule_init]
    fn pyinit(m: &Bound<'_, PyModule>) -> PyResult<()> {
        m.add_class::<TboExportContext>()?;
        m.add_class::<TBOHierarchy>()?;
        m.add_class::<TboImportContext>()?;
        m.add_class::<HierarchicalEntity>()?;
        Ok(())
    }

    #[pyfunction]
    fn group_all_objects_command(_py: Python) -> () {
        let _ = engine_api::group_all_objects_command()
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()));
    }

    #[pyfunction]
    fn tbo_export_command(
        slab_scene_name: Vec<u8>,
        slab_asset_name: Vec<u8>,
        slab_fragment_name: Vec<u8>,
        scene_data_ptr: u64,
        scene_offset_ptr: u64,
        scene_remaining: u64,
        asset_data_ptr: u64,
        asset_offset_ptr: u64,
        asset_remaining: u64,
        frag_data_ptr: u64,
        frag_offset_ptr: u64,
        frag_remaining: u64,
        scene_transform: bool,
        scene_similarity: bool,
        asset_embedding: bool,
        asset_transform: bool,
        fragment_xyz: bool,
        normal_variance: bool,
        surface_variation: bool,
        combined: bool,
        target_point_count: u32,
    ) -> PyResult<(u64, u64, u64, u64, u64, u64)> {
        let slab_scene: [u8; 64] = slab_scene_name.try_into()
            .map_err(|_| PyErr::new::<pyo3::exceptions::PyValueError, _>("slab_scene_name must be 64 bytes"))?;
        let slab_asset: [u8; 64] = slab_asset_name.try_into()
            .map_err(|_| PyErr::new::<pyo3::exceptions::PyValueError, _>("slab_asset_name must be 64 bytes"))?;
        let slab_fragment: [u8; 64] = slab_fragment_name.try_into()
            .map_err(|_| PyErr::new::<pyo3::exceptions::PyValueError, _>("slab_fragment_name must be 64 bytes"))?;
        let resp = engine_api::tbo_export_command(
            &slab_scene, &slab_asset, &slab_fragment,
            scene_data_ptr, scene_offset_ptr, scene_remaining,
            asset_data_ptr, asset_offset_ptr, asset_remaining,
            frag_data_ptr, frag_offset_ptr, frag_remaining,
            scene_transform, scene_similarity,
            asset_embedding, asset_transform,
            fragment_xyz, normal_variance,
            surface_variation, combined,
            target_point_count,
        )
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e))?;
        if resp.header.status != 0 {
            return Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                "tbo_export: buffer full - insufficient space in export buffer".to_string(),
            ));
        }
        let result = resp.read_tbo_export_response()
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                format!("Failed to read export response: {}", e),
            ))?;
        Ok(result)
    }
}
