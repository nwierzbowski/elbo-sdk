//! TBO Hierarchy - unified access to the Scene -> Asset -> Fragment -> Points hierarchy.
//!
//! Child transitions are stored in a map, allowing new levels to be added
//! without Rust code changes.

use pyo3::prelude::*;
use std::collections::HashMap;

use super::tbo_collection::CollectionState;
use super::tbo_entity::HierarchicalEntity;

#[derive(Clone, Copy, PartialEq)]
pub enum StateRef {
    Scenes,
    Assets,
    Fragments,
}

impl StateRef {
    pub fn get_state<'a>(&self, h: &'a TBOHierarchy) -> &'a CollectionState {
        match self {
            StateRef::Scenes => &h.scenes_state,
            StateRef::Assets => &h.assets_state,
            StateRef::Fragments => &h.fragments_state,
        }
    }
}

pub struct ChildInfo {
    pub entity_idx: usize,
    pub child_offset: usize,
    pub child_count: usize,
    pub state: StateRef,
}

#[pyclass(unsendable)]
pub struct TBOHierarchy {
    pub scenes_state: CollectionState,
    pub assets_state: CollectionState,
    pub fragments_state: CollectionState,
    child_transitions: HashMap<&'static str, (StateRef, StateRef)>,
}

#[pymethods]
impl TBOHierarchy {
    #[getter(Scenes)]
    fn scenes(slf: PyRef<Self>, py: Python) -> HierarchicalEntity {
        let keepalive = unsafe {
            let ptr = slf.as_ptr();
            pyo3::ffi::Py_IncRef(ptr);
            Py::from_owned_ptr(py, ptr)
        };
        let total = slf.scenes_state.total_entities();
        HierarchicalEntity::new(
            py, keepalive,
            ChildInfo { entity_idx: 0, child_offset: 0, child_count: total, state: StateRef::Scenes },
            None, 0, Vec::new(),
        )
    }

    #[getter(Assets)]
    fn assets(slf: PyRef<Self>, py: Python) -> HierarchicalEntity {
        let keepalive = unsafe {
            let ptr = slf.as_ptr();
            pyo3::ffi::Py_IncRef(ptr);
            Py::from_owned_ptr(py, ptr)
        };
        let total = slf.assets_state.total_entities();
        HierarchicalEntity::new(
            py, keepalive,
            ChildInfo { entity_idx: 0, child_offset: 0, child_count: total, state: StateRef::Assets },
            None, 0, Vec::new(),
        )
    }

    #[getter(Fragments)]
    fn fragments(slf: PyRef<Self>, py: Python) -> HierarchicalEntity {
        let keepalive = unsafe {
            let ptr = slf.as_ptr();
            pyo3::ffi::Py_IncRef(ptr);
            Py::from_owned_ptr(py, ptr)
        };
        let total = slf.fragments_state.total_entities();
        HierarchicalEntity::new(
            py, keepalive,
            ChildInfo { entity_idx: 0, child_offset: 0, child_count: total, state: StateRef::Fragments },
            None, 0, Vec::new(),
        )
    }

    #[getter(scene_count)]
    fn scene_count(&self) -> usize {
        self.scenes_state.total_entities()
    }

    #[getter(asset_count)]
    fn asset_count(&self) -> usize {
        self.assets_state.total_entities()
    }

    #[getter(fragment_count)]
    fn fragment_count(&self) -> usize {
        self.fragments_state.total_entities()
    }
}

impl TBOHierarchy {
    pub fn new(
        scenes: CollectionState,
        assets: CollectionState,
        fragments: CollectionState,
    ) -> Self {
        let mut child_transitions = HashMap::new();
        child_transitions.insert("Assets", (StateRef::Scenes, StateRef::Assets));
        child_transitions.insert("Fragments", (StateRef::Assets, StateRef::Fragments));
        Self {
            scenes_state: scenes,
            assets_state: assets,
            fragments_state: fragments,
            child_transitions,
        }
    }

    pub fn resolve_child(&self, name: &str, parent_idx: usize, caller_state: StateRef) -> Option<ChildInfo> {
        let (parent_state_ref, child_state_ref) = *self.child_transitions.get(name)?;
        if parent_state_ref != caller_state {
            return None;
        }
        let parent_state = parent_state_ref.get_state(self);
        let child_offset = parent_state.cumulative_rows_before(parent_idx).ok()?;
        let child_count = parent_state.entity_row_count(parent_idx).ok()?;
        if child_count == 0 {
            return None;
        }
        Some(ChildInfo {
            entity_idx: child_offset,
            child_offset,
            child_count,
            state: child_state_ref,
        })
    }
}
