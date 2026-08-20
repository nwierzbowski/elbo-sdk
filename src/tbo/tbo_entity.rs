//! Single entity type for hierarchical TBO navigation.
//!
//! A `HierarchicalEntity` is a cursor over one state (scenes, assets,
//! fragments, or points) of a `TBOHierarchy`. It tracks the selected entity
//! inside its range (child offset + local index) and exposes the selected
//! entity's rows: one f32 per (row, channel). `get_child(name)` descends one
//! level using the hierarchy's transition set.
//!
//! Indexing (`__getitem__`) and iteration (`__next__`) return *snapshots*:
//! independent cursors whose resolved data windows were captured at that
//! moment.

use pyo3::ffi;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyFloat};

use super::tbo_data_view::ChannelSet;
use super::tbo_hierarchy::{ChildInfo, StateRef, TBOHierarchy};
use super::{py_attr_error, py_index_error, py_value_error};

/// The selected entity's resolved data window (zero-copy into backing memory).
#[derive(Clone)]
struct Resolved {
    data_ptr: *const u8,
    data_len: usize,
    row_stride: usize,
    channels: ChannelSet,
    bytes_per_element: usize,
}

impl Resolved {
    fn row_count(&self) -> usize {
        let row_bytes = self.bytes_per_element * self.row_stride;
        self.data_len.checked_div(row_bytes).unwrap_or(0)
    }
}

/// Access to the parent entity's selected row, carried one level down so
/// children can read parent attributes (e.g. `asset.sx`) by channel name.
#[derive(Clone)]
struct ParentRow {
    base_ptr: *const u8,
    row_stride: usize,
    channels: ChannelSet,
    row_ptr: *const u8,
    bytes_per_element: usize,
}

struct EntityInner {
    hierarchy_ref: Py<TBOHierarchy>,
    /// Range of child entities this cursor navigates.
    child: ChildInfo,
    /// Position within the range; `usize::MAX` marks a freshly created iterator.
    local_idx: usize,
    /// Global index of the selected entity within `child.state`.
    entity_idx: usize,
    resolved: Option<Resolved>,
    parent: Option<ParentRow>,
}

impl EntityInner {
    /// Copy this cursor (refcounting the hierarchy keepalive).
    fn snapshot(&self, py: Python) -> Self {
        Self {
            hierarchy_ref: self.hierarchy_ref.clone_ref(py),
            child: self.child,
            local_idx: self.local_idx,
            entity_idx: self.entity_idx,
            resolved: self.resolved.clone(),
            parent: self.parent.clone(),
        }
    }
}

impl EntityInner {
    fn new(
        py: Python,
        hierarchy_ref: Py<TBOHierarchy>,
        child: ChildInfo,
        parent: Option<ParentRow>,
    ) -> Self {
        let mut this = Self {
            hierarchy_ref,
            child,
            local_idx: 0,
            entity_idx: child.offset,
            resolved: None,
            parent,
        };
        this.resolve(py);
        this
    }

    /// Positioned, resolved snapshot cursor at `local_idx`.
    fn at(&self, py: Python, local_idx: usize) -> Self {
        let mut this = Self {
            hierarchy_ref: self.hierarchy_ref.clone_ref(py),
            child: self.child,
            local_idx,
            entity_idx: self.child.offset + local_idx,
            resolved: None,
            parent: self.parent.clone(),
        };
        this.resolve(py);
        this
    }

    /// Iterator cursor: same range, positioned before the first entity.
    fn iterator(&self, py: Python) -> Self {
        let mut this = self.snapshot(py);
        this.local_idx = usize::MAX;
        this.entity_idx = this.child.offset;
        this.resolved = None;
        this
    }

    /// Resolve the currently selected entity, plus the parent's selected row.
    fn resolve(&mut self, py: Python) {
        if self.child.state == StateRef::SampledPoints {
            // Terminal level: a sampled point is one row of the parent fragment.
            let parent = self
                .parent
                .as_ref()
                .expect("SampledPoints always have a parent");
            let row_bytes = parent.bytes_per_element * parent.row_stride;
            self.resolved = Some(Resolved {
                data_ptr: unsafe { parent.base_ptr.add(self.local_idx * row_bytes) },
                data_len: row_bytes,
                row_stride: parent.row_stride,
                channels: parent.channels.clone(),
                bytes_per_element: parent.bytes_per_element,
            });
        } else {
            let resolved = {
                let h = self.hierarchy_ref.borrow(py);
                self.child.state
                    .get_state(&h)
                    .and_then(|s| s.resolve_entity(self.entity_idx).ok())
            };
            self.resolved = resolved.map(|(data_ptr, data_len, row_stride, channels, bytes_per_element)| Resolved {
                data_ptr,
                data_len,
                row_stride,
                channels,
                bytes_per_element,
            });
        }
        if let Some(parent) = self.parent.as_mut() {
            let row_bytes = parent.bytes_per_element * parent.row_stride;
            parent.row_ptr = unsafe { parent.base_ptr.add(self.local_idx * row_bytes) };
        }
    }

    fn row_count(&self) -> usize {
        self.resolved.as_ref().map(Resolved::row_count).unwrap_or(0)
    }

    fn require_data(&self) -> PyResult<()> {
        match &self.resolved {
            Some(resolved) if resolved.row_stride > 0 => Ok(()),
            _ => Err(py_value_error("no data for selected entity")),
        }
    }
}

#[pyclass(unsendable)]
pub struct HierarchicalEntity {
    inner: EntityInner,
}

#[pymethods]
impl HierarchicalEntity {
    /// Number of rows of the selected entity.
    #[getter(row_count)]
    fn row_count(&self) -> usize {
        self.inner.row_count()
    }

    #[getter(channel_names)]
    fn channel_names(&self) -> Vec<String> {
        self.inner
            .resolved
            .as_ref()
            .map(|r| r.channels.names().clone())
            .unwrap_or_default()
    }

    #[getter(selected_entity_idx)]
    fn selected_entity_idx(&self) -> usize {
        self.inner.local_idx
    }

    #[setter(selected_entity_idx)]
    fn set_selected_entity_idx(mut slf: PyRefMut<Self>, idx: usize) -> PyResult<()> {
        let py = slf.py();
        if idx >= slf.inner.child.count {
            return Err(py_index_error("Entity index", idx, slf.inner.child.count));
        }
        slf.inner.local_idx = idx;
        slf.inner.entity_idx = slf.inner.child.offset + idx;
        slf.inner.resolve(py);
        Ok(())
    }

    fn __repr__(&self) -> String {
        format!(
            "<HierarchicalEntity idx={} rows={} channels={}>",
            self.inner.local_idx,
            self.inner.row_count(),
            self.inner
                .resolved
                .as_ref()
                .map(|r| r.channels.name_count())
                .unwrap_or(0)
        )
    }

    fn __len__(&self) -> usize {
        self.inner.child.count
    }

    fn __iter__(slf: PyRef<Self>) -> PyResult<Py<Self>> {
        let py = slf.py();
        Py::new(py, Self { inner: slf.inner.iterator(py) })
    }

    /// Advance the iterator; each advance returns an independent snapshot of
    /// the newly selected entity.
    fn __next__(mut slf: PyRefMut<Self>) -> Option<PyResult<Py<Self>>> {
        let py = slf.py();
        slf.inner.local_idx = slf.inner.local_idx.wrapping_add(1);
        if slf.inner.local_idx >= slf.inner.child.count {
            return None;
        }
        slf.inner.entity_idx = slf.inner.child.offset + slf.inner.local_idx;
        slf.inner.resolve(py);
        Some(Py::new(py, Self { inner: slf.inner.snapshot(py) }))
    }

    fn __getitem__(slf: PyRef<Self>, idx: usize) -> PyResult<Py<Self>> {
        let py = slf.py();
        if idx >= slf.inner.child.count {
            return Err(py_index_error("Index", idx, slf.inner.child.count));
        }
        Py::new(py, Self {
            inner: slf.inner.at(py, idx),
        })
    }

    /// Parent-row scalar for the selected entity, addressed by channel name.
    /// Raises AttributeError for unknown names or when the entity has no parent.
    /// Parent rows are always f32-backed (scene/asset/fragment).
    fn __getattr__(slf: PyRef<Self>, name: &str) -> PyResult<Py<PyAny>> {
        let parent = slf.inner.parent.as_ref().ok_or_else(|| py_attr_error(name))?;
        let ch_idx = parent.channels.index(name).ok_or_else(|| py_attr_error(name))?;
        if parent.row_ptr.is_null() {
            return Err(py_attr_error(name));
        }
        let val = unsafe {
            *(parent.row_ptr.add(ch_idx * parent.bytes_per_element) as *const f32)
        };
        Ok(PyFloat::new(slf.py(), val as f64).into())
    }

    /// All rows of one channel of the selected entity, as a 1-D array.
    /// f32-backed formats (scene/asset/fragment/points) yield float32; the
    /// u64 Faces format yields int64.
    fn channel(slf: PyRef<Self>, py: Python, name: &str) -> PyResult<Py<PyAny>> {
        let ch_idx = slf
            .inner
            .resolved
            .as_ref()
            .and_then(|r| r.channels.index(name))
            .ok_or_else(|| py_value_error(format!("Unknown channel: {name}")))?;
        slf.inner.require_data()?;
        let resolved = slf.inner.resolved.as_ref().expect("require_data checked");
        let rows = resolved.row_count();
        let base = resolved.data_ptr;
        let stride = resolved.row_stride;
        let bytes_per_elem = resolved.bytes_per_element;
        if bytes_per_elem == 8 {
            let mut out = Vec::with_capacity(rows);
            for r in 0..rows {
                let val = unsafe { *(base.add(r * stride * bytes_per_elem + ch_idx * bytes_per_elem) as *const i64) };
                out.push(val);
            }
            let arr = numpy::PyArray1::<i64>::from_vec(py, out);
            Ok(arr.into_any().unbind())
        } else {
            let mut out = Vec::with_capacity(rows);
            for r in 0..rows {
                let val = unsafe { *(base.add(r * stride * bytes_per_elem + ch_idx * bytes_per_elem) as *const f32) };
                out.push(val);
            }
            let arr = numpy::PyArray1::<f32>::from_vec(py, out);
            Ok(arr.into_any().unbind())
        }
    }

    /// Read-only memoryview of one row of the selected entity.
    /// The row is copied into Python-owned bytes, so the view is safe
    /// even if the hierarchy is later dropped or the buffer is reset.
    fn row(slf: PyRef<Self>, py: Python, idx: usize) -> PyResult<Py<PyAny>> {
        slf.inner.require_data()?;
        let rows = slf.inner.row_count();
        if idx >= rows {
            return Err(py_index_error("Row index", idx, rows));
        }
        let resolved = slf.inner.resolved.as_ref().expect("require_data checked");
        let row_bytes_count = resolved.row_stride * resolved.bytes_per_element;
        let ptr = unsafe { resolved.data_ptr.add(idx * row_bytes_count) };
        let row_bytes =
            unsafe { std::slice::from_raw_parts(ptr, row_bytes_count) };
        let bytes = PyBytes::new(py, row_bytes);
        let mv = unsafe { ffi::PyMemoryView_FromObject(bytes.as_ptr()) };
        if mv.is_null() {
            return Err(super::py_runtime_error("failed to create memoryview from row"));
        }
        let view: Py<PyAny> = unsafe { Py::from_owned_ptr(py, mv) };
        Ok(view)
    }

    #[pyo3(name = "get_child")]
    fn get_child(slf: PyRef<Self>, py: Python, name: &str) -> Option<Self> {
        let h = slf.inner.hierarchy_ref.borrow(py);
        let info = h
            .resolve_child(name, slf.inner.entity_idx, slf.inner.child.state)?;
        drop(h);

        let parent = slf.inner.resolved.as_ref().map(|r| ParentRow {
            base_ptr: r.data_ptr,
            row_stride: r.row_stride,
            channels: r.channels.clone(),
            row_ptr: std::ptr::null(),
            bytes_per_element: r.bytes_per_element,
        });
        Some(Self {
            inner: EntityInner::new(py, slf.inner.hierarchy_ref.clone_ref(py), info, parent),
        })
    }
}

impl HierarchicalEntity {
    pub fn new(
        py: Python,
        hierarchy_ref: Py<TBOHierarchy>,
        child_info: ChildInfo,
    ) -> Self {
        Self {
            inner: EntityInner::new(py, hierarchy_ref, child_info, None),
        }
    }
}
