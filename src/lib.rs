mod asset_sync_context;
mod command_thread;
mod engine_api;
mod engine_client; // This line remains unchanged
mod mesh_sync_thread;
extern crate iceoryx2_loggers;

use pyo3::prelude::*;

#[pymodule(name = "_elbo_sdk_rust")]
mod elbo_sdk_rust {
    use crate::asset_sync_context::AssetSyncContext;
    use crate::engine_api;
    use pivot_com_types::fields::Uuid;
    use pyo3::prelude::*;
    use std::path::PathBuf;

    #[pyfunction]
    fn start_engine(py: Python) -> PyResult<()> {
        py.detach(|| {
            engine_api::start_engine()
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))
        })
    }

    #[pyfunction]
    fn stop_engine(py: Python) -> PyResult<()> {
        py.detach(|| {
            engine_api::stop_engine()
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))
        })
    }

    #[pyfunction]
    fn standardize_synced_groups_command(
        py: Python,
        uuids: Vec<Uuid>,
        surface_contexts: Vec<u32>,
    ) -> () {
        py.detach(|| {
            let _ = engine_api::standardize_synced_groups_command(uuids, surface_contexts)
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()));
        });
    }

    #[pyfunction]
    fn set_surface_types_command(
        py: Python,
        group_surface_map: std::collections::HashMap<Uuid, i64>,
    ) -> () {
        py.detach(|| {
            let _ = engine_api::set_surface_types_command(group_surface_map)
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()));
        });
    }

    #[pyfunction]
    fn drop_groups_command(py: Python, uuids: Vec<Uuid>) -> () {
        py.detach(|| {
            let _ = engine_api::drop_groups_command(uuids)
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()));
        });
    }

    #[pyfunction]
    fn get_surface_types_command(py: Python) -> () {
        py.detach(|| {
            let _ = engine_api::get_surface_types_command()
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()));
        });
    }

    #[pyfunction]
    fn organize_objects_command(py: Python) -> () {
        py.detach(|| {
            let _ = engine_api::organize_objects_command()
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()));
        });
    }

    #[pyfunction]
    fn extract_geometric_features_command(py: Python, uuids: Vec<Uuid>) -> () {
        py.detach(|| {
            let _ = engine_api::extract_geometric_features_command(uuids)
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()));
        });
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
    fn standardize_groups_command(py: Python, uuids: Vec<Uuid>) -> () {
        py.detach(|| {
            let _ = engine_api::standardize_groups_command(uuids)
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()));
        });
    }

    #[pyfunction]
    fn generate_uuid_bytes() -> PyResult<[u8; 16]> {
        Ok(engine_api::generate_uuid_bytes())
    }

    #[pyfunction]
    fn export_assets_command(
        py: Python,
        path: String,
        target_bytes: u64,
        uuids: Vec<Uuid>,
    ) -> () {
        py.detach(|| {
            let _ = engine_api::export_assets_command(&path, target_bytes, uuids)
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()));
        });
    }

    #[pyfunction]
    fn export_all_command(py: Python, path: String, target_bytes: u64) -> () {
        py.detach(|| {
            let _ = engine_api::export_all_command(&path, target_bytes)
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()));
        });
    }

    #[pyfunction]
    fn export_tbo_command(
        py: Python,
        path: String,
        target_bytes: u64,
        flags: u32,
        uuids: Vec<Uuid>,
    ) -> () {
        py.detach(|| {
            let _ = engine_api::export_tbo_command(&path, target_bytes, flags, uuids)
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()));
        });
    }

    #[pyfunction]
    fn import_assets_command(py: Python, paths: Vec<String>) -> () {
        py.detach(|| {
            let _ = engine_api::import_assets_command(paths)
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()));
        });
    }
}
