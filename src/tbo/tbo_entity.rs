//! Single entity type for hierarchical TBO navigation.
//!
//! Uses `get_child(name: &str)` for extensible child navigation, with the
//! hierarchy's transition map determining valid child types.

use pyo3::prelude::*;

use super::tbo_hierarchy::{TBOHierarchy, StateRef, ChildInfo};

struct EntityInner {
    hierarchy_ref: Py<TBOHierarchy>,
    entity_idx: usize,
    local_idx: usize,
    data_ptr: *const f32,
    data_len: usize,
    row_stride: usize,
    channel_names: Vec<String>,
    parent_data_ptr: Option<*const f32>,
    parent_row_stride: usize,
    parent_row_ptr: *const f32,
    parent_channel_names: Vec<String>,
    child_offset: usize,
    child_count: usize,
    child_state: StateRef,
    _keepalive: Py<PyAny>,
}

fn keep_hierarchy(py: Python, hierarchy_ref: &Py<TBOHierarchy>) -> Py<PyAny> {
    unsafe {
        let ptr = hierarchy_ref.as_ptr();
        pyo3::ffi::Py_IncRef(ptr);
        Py::<TBOHierarchy>::from_owned_ptr(py, ptr).into_any()
    }
}

fn child_hierarchy_ref(py: Python, hierarchy_ref: &Py<TBOHierarchy>) -> Py<TBOHierarchy> {
    unsafe {
        let ptr = hierarchy_ref.as_ptr();
        pyo3::ffi::Py_IncRef(ptr);
        Py::from_owned_ptr(py, ptr)
    }
}

impl EntityInner {
    fn new(
        py: Python,
        hierarchy_ref: Py<TBOHierarchy>,
        child_info: ChildInfo,
        parent_data_ptr: Option<*const f32>,
        parent_row_stride: usize,
        parent_channel_names: Vec<String>,
    ) -> Self {
        let keepalive = keep_hierarchy(py, &hierarchy_ref);
        let (data_ptr, data_len, row_stride, channel_names) =
            resolve_from_state(child_info.state, &hierarchy_ref, child_info.entity_idx);
        let parent_row_ptr = parent_data_ptr.unwrap_or(std::ptr::null());
        Self {
            hierarchy_ref,
            entity_idx: child_info.entity_idx,
            local_idx: 0,
            data_ptr, data_len, row_stride, channel_names,
            parent_data_ptr, parent_row_stride, parent_row_ptr,
            parent_channel_names,
            child_offset: child_info.child_offset,
            child_count: child_info.child_count,
            child_state: child_info.state,
            _keepalive: keepalive,
        }
    }

    fn update_resolved(&mut self) {
        let resolved = resolve_from_state(self.child_state, &self.hierarchy_ref, self.entity_idx);
        let (data_ptr, data_len, row_stride, channel_names) = resolved;
        self.data_ptr = data_ptr;
        self.data_len = data_len;
        self.row_stride = row_stride;
        self.channel_names = channel_names;
        if let Some(base) = self.parent_data_ptr {
            self.parent_row_ptr = unsafe { base.add(self.local_idx * self.parent_row_stride) };
        }
    }

}

fn resolve_from_state(
    state_ref: StateRef,
    hierarchy_ref: &Py<TBOHierarchy>,
    entity_idx: usize,
) -> (*const f32, usize, usize, Vec<String>) {
    let py = unsafe { Python::assume_attached() };
    let h = hierarchy_ref.borrow(py);
    let state = state_ref.get_state(&h);
    match state.resolve_entity(entity_idx) {
        Ok((dp, dl, rs, cn)) => (dp, dl, rs, cn.clone()),
        Err(_) => (std::ptr::null(), 0usize, 0usize, Vec::<String>::new()),
    }
}

// ============================================================================
// HierarchicalEntity
// ============================================================================

#[pyclass(unsendable)]
pub struct HierarchicalEntity {
    inner: EntityInner,
}

#[pymethods]
impl HierarchicalEntity {
    #[getter(entity_count)]
    fn entity_count(&self) -> PyResult<usize> {
        if self.inner.data_len == 0 || self.inner.row_stride == 0 {
            return Ok(0);
        }
        Ok(self.inner.data_len / self.inner.row_stride)
    }

    #[getter(channel_names)]
    fn channel_names(&self) -> Vec<String> {
        self.inner.channel_names.clone()
    }

    #[getter(selected_entity_idx)]
    fn selected_entity_idx(&self) -> usize {
        self.inner.entity_idx
    }

    #[setter(selected_entity_idx)]
    fn set_selected_entity_idx(mut slf: PyRefMut<Self>, idx: usize) {
        slf.inner.entity_idx = idx;
        slf.inner.update_resolved();
    }

    fn __len__(&self) -> usize {
        self.inner.child_count
    }

    fn __iter__(mut slf: PyRefMut<Self>) -> Py<Self> {
        slf.inner.local_idx = usize::MAX;
        unsafe {
            let ptr = slf.as_ptr();
            pyo3::ffi::Py_IncRef(ptr);
            Py::from_owned_ptr(slf.py(), ptr)
        }
    }

    fn __next__(mut slf: PyRefMut<Self>) -> Option<Py<Self>> {
        slf.inner.local_idx = slf.inner.local_idx.wrapping_add(1);
        if slf.inner.local_idx >= slf.inner.child_count {
            return None;
        }
        slf.inner.entity_idx = slf.inner.child_offset + slf.inner.local_idx;
        slf.inner.update_resolved();
        let py = slf.py();
        unsafe {
            let ptr = slf.as_ptr();
            pyo3::ffi::Py_IncRef(ptr);
            Some(Py::from_owned_ptr(py, ptr))
        }
    }

    fn __getitem__(mut slf: PyRefMut<Self>, idx: usize) -> PyResult<Py<Self>> {
        if idx >= slf.inner.child_count {
            return Err(PyErr::new::<pyo3::exceptions::PyIndexError, _>(
                format!("Index {} out of range (0..{})", idx, slf.inner.child_count)
            ));
        }
        slf.inner.local_idx = idx;
        slf.inner.entity_idx = slf.inner.child_offset + idx;
        slf.inner.update_resolved();
        unsafe {
            let ptr = slf.as_ptr();
            pyo3::ffi::Py_IncRef(ptr);
            Ok(Py::from_owned_ptr(slf.py(), ptr))
        }
    }

    fn __getattr__(slf: PyRef<Self>, name: &str) -> PyResult<Py<PyAny>> {
        if slf.inner.parent_data_ptr.is_none() {
            return Ok(slf.py().None());
        }
        if let Some(ch_idx) = slf.inner.parent_channel_names.iter().position(|n| n == name) {
            let val = unsafe { *slf.inner.parent_row_ptr.add(ch_idx) };
            return Ok(pyo3::types::PyFloat::new(slf.py(), val as f64).into());
        }
        Ok(slf.py().None())
    }

    #[pyo3(name = "get_child")]
    fn get_child(slf: PyRef<Self>, py: Python, name: &str) -> Option<Self> {
        let h = slf.inner.hierarchy_ref.borrow(py);
        let info = h.resolve_child(name, slf.inner.entity_idx, slf.inner.child_state)?;
        drop(h);
        Some(Self::new(
            py,
            child_hierarchy_ref(py, &slf.inner.hierarchy_ref),
            info,
            Some(slf.inner.data_ptr),
            slf.inner.row_stride,
            slf.inner.channel_names.clone(),
        ))
    }
}

impl HierarchicalEntity {
    pub fn new(
        py: Python,
        hierarchy_ref: Py<TBOHierarchy>,
        child_info: ChildInfo,
        parent_data_ptr: Option<*const f32>,
        parent_row_stride: usize,
        parent_channel_names: Vec<String>,
    ) -> Self {
        let inner = EntityInner::new(
            py, hierarchy_ref, child_info,
            parent_data_ptr, parent_row_stride, parent_channel_names,
        );
        Self { inner }
    }
}
