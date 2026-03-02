mod asset_sync_context;
mod command_thread;
mod engine_api;
mod engine_client; // This line remains unchanged
mod mesh_sync_thread;
extern crate iceoryx2_loggers;

use pyo3::prelude::*;

#[pymodule(name = "_elbo_sdk_rust")]
mod elbo_sdk_rust {
    use pivot_com_types::fields::Uuid;
    use pyo3::prelude::*;
    use std::path::PathBuf;
    use crate::asset_sync_context::AssetSyncContext;
    use crate::engine_api;

    #[pyfunction]
    fn start_engine() -> PyResult<()> {
        engine_api::start_engine()
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))
    }

    #[pyfunction]
    fn stop_engine() -> PyResult<()> {
        engine_api::stop_engine()
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))
    }

    #[pyfunction]
    fn standardize_synced_groups_command(
        uuids: Vec<Uuid>,
        surface_contexts: Vec<u32>,
    ) -> () {
        let _ = engine_api::standardize_synced_groups_command(uuids, surface_contexts)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()));
    }

    #[pyfunction]
    fn set_surface_types_command(group_surface_map: std::collections::HashMap<Uuid, i64>) -> () {
        let _ = engine_api::set_surface_types_command(group_surface_map)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()));
    }

    #[pyfunction]
    fn drop_groups_command(uuids: Vec<Uuid>) -> () {
        let _ = engine_api::drop_groups_command(uuids)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()));
    }

    #[pyfunction]
    fn get_surface_types_command() -> () {
        let _ = engine_api::get_surface_types_command()
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()));
    }

    #[pyfunction]
    fn organize_objects_command() -> () {
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
    fn prepare_standardize_groups(
        vert_counts: Vec<u32>,
        edge_counts: Vec<u32>,
        loop_counts: Vec<u32>,
        total_loop_lengths: Vec<u32>,
        object_counts: Vec<u32>,
        group_names: Vec<String>,
        surface_contexts: Vec<u16>,
        asset_uuids: Vec<Uuid>,
    ) -> PyResult<AssetSyncContext> {
        let context = engine_api::allocate_memory(
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

        Ok(context)
    }

    #[pyfunction]
    fn generate_uuid_bytes() -> PyResult<[u8; 16]> {
        Ok(engine_api::generate_uuid_bytes())
    }
}
