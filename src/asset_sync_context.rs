use pivot_com_types::{
    asset_meta::{AssetDataSlices, AssetMeta},
    asset_ptr::AssetPtr, fields::Uuid,
};
use pyo3::{ffi, prelude::*, types::PyByteArray};
use std::{os::raw::c_char, ptr::NonNull};

use crate::engine_api;

#[pyclass(unsendable)]
pub struct AssetSyncContext {
    asset_slices: Vec<AssetDataSlices>,
    asset_ptrs: Vec<AssetPtr>,
    asset_uuids: Vec<Uuid>,
    asset_surface_contexts: Vec<u16>
}

impl AssetSyncContext {
    pub fn new(ptrs: Vec<NonNull<AssetMeta>>, asset_ptrs: &[AssetPtr]) -> AssetSyncContext {
        let mut asset_slices = Vec::with_capacity(ptrs.len());
        let mut asset_uuids = Vec::with_capacity(ptrs.len());
        let mut asset_surface_contexts = Vec::with_capacity(ptrs.len());

        for mut ptr in ptrs {
            asset_slices.push(unsafe { ptr.as_mut().get_slices() });
            asset_uuids.push(unsafe {ptr.as_mut().uuid});
            asset_surface_contexts.push(unsafe {ptr.as_mut().surface_context});
        }

        AssetSyncContext {
            asset_slices,
            asset_ptrs: asset_ptrs.to_vec(),
            asset_uuids,
            asset_surface_contexts,
        }
    }
}

#[pymethods]
impl AssetSyncContext {
    pub fn uuids(&self, py: Python) -> PyResult<Py<PyAny>> {
        let flattened: Vec<u8> = self.asset_uuids.iter()
            .flat_map(|uuid| uuid.bytes.iter())
            .copied()
            .collect();

        Ok(PyByteArray::new(py, &flattened).into_any().unbind())
    }

    pub fn surface_contexts(&self, py: Python) -> PyResult<Py<PyAny>> {
        let flattened: Vec<u8> = self.asset_surface_contexts.iter()
            .flat_map(|c| c.to_le_bytes())
            .collect();

        Ok(PyByteArray::new(py, &flattened).into_any().unbind())
    }

    pub fn buffers(
        &self,
        py: Python,
        i: usize,
    ) -> PyResult<(
        Py<PyAny>,
        Py<PyAny>,
        Py<PyAny>,
        Py<PyAny>,
        Py<PyAny>,
        Py<PyAny>,
        Py<PyAny>,
        Py<PyAny>,
        Py<PyAny>,
        Py<PyAny>,
    )> {
        let g = self.asset_slices.get(i).ok_or_else(|| {
            PyErr::new::<pyo3::exceptions::PyIndexError, _>(format!("index {} out of range", i))
        })?;

        // Tuple order: (obj_uuids, verts, edges, loops, loop_bases, object_loop_counts, transforms, vert_counts, edge_counts, object_names)
        let obj_uuids = memoryview_from_slice(py, g.0)?;
        let verts = memoryview_from_slice(py, g.1)?;
        let edges = memoryview_from_slice(py, g.2)?;
        let loops = memoryview_from_slice(py, g.3)?;
        let loop_bases = memoryview_from_slice(py, g.4)?;
        let object_loop_counts = memoryview_from_slice(py, g.5)?;
        let transforms = memoryview_from_slice(py, g.6)?;
        let vert_counts = memoryview_from_slice(py, g.7)?;
        let edge_counts = memoryview_from_slice(py, g.8)?;
        let object_names = memoryview_from_slice(py, g.9)?;

        Ok((
            verts,
            edges,
            loops,
            loop_bases,
            object_loop_counts,
            transforms,
            vert_counts,
            edge_counts,
            object_names,
            obj_uuids,
        ))
    }

    pub fn size(&self) -> usize {
        self.asset_slices.len()
    }

    pub fn send(&mut self) -> () {
        let response = engine_api::send_mesh_command(std::mem::take(&mut self.asset_ptrs));

        if response.is_err() {
            println!("{:?}", response.err());
        }
    }
}

fn memoryview_from_slice(py: Python, slice_ptr: *mut [u8]) -> PyResult<Py<PyAny>> {
    let ptr = slice_ptr as *mut u8 as *mut c_char;
    let len = unsafe { (&*slice_ptr).len() } as isize;
    let mv = unsafe { ffi::PyMemoryView_FromMemory(ptr, len, ffi::PyBUF_WRITE) };
    if mv.is_null() {
        return Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
            "failed to create memoryview from slice",
        ));
    }
    Ok(unsafe { Py::from_owned_ptr(py, mv) })
}

