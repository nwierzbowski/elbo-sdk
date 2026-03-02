use pivot_com_types::{asset_meta::{AssetDataSlices, AssetMeta}, asset_ptr::AssetPtr};
use pyo3::{ffi, prelude::*};
use std::{os::raw::c_char, ptr::NonNull};

use crate::engine_api;



#[pyclass(unsendable)]
pub struct AssetSyncContext {
    asset_slices: Vec<AssetDataSlices>,
    asset_ptrs: Option<Vec<AssetPtr>>,
}

impl AssetSyncContext {
    pub fn new(ptrs: Vec<NonNull<AssetMeta>>, asset_ptrs:  &[AssetPtr]) -> AssetSyncContext {
        let mut asset_slices = Vec::with_capacity(ptrs.len());

        for mut ptr in ptrs {
            asset_slices.push(unsafe{ ptr.as_mut().get_slices()})
        };

        AssetSyncContext { asset_slices, asset_ptrs: Some(asset_ptrs.to_vec()) }
    }
}

#[pymethods]
impl AssetSyncContext {
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
    )> {
        let g = self.asset_slices.get(i).ok_or_else(|| {
            PyErr::new::<pyo3::exceptions::PyIndexError, _>(format!("index {} out of range", i))
        })?;

        // Tuple order: (obj_uuids, verts, edges, loops, loop_bases, object_loop_counts, transforms, vert_counts, edge_counts, object_names)
        let obj_uuids = memoryview_from_slice(py, g.0)?;
        let v = memoryview_from_slice(py, g.1)?;
        let e = memoryview_from_slice(py, g.2)?;
        let t = memoryview_from_slice(py, g.6)?;
        let vc = memoryview_from_slice(py, g.7)?;
        let ec = memoryview_from_slice(py, g.8)?;
        let on = memoryview_from_slice(py, g.9)?;

        Ok((v, e, t, vc, ec, on, obj_uuids))
    }

    pub fn size(&self) -> usize {
        self.asset_slices.len()
    }

    pub fn finalize(&mut self) -> () {
        let ptrs = match std::mem::take(&mut self.asset_ptrs) {
            Some(ptrs) => ptrs,
            None => return,
        };
        let response = engine_api::standardize_groups_command(ptrs);

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
