//! TBO Hierarchy - unified access to the
//! Scene -> Asset -> {Fragment, Points, Faces} -> SampledPoints hierarchy.
//!
//! Fragments, Points and Faces are siblings (children of Assets), aligned
//! per-fragment in the same order. SampledPoints is a terminal child of a
//! Fragment exposing that fragment's downsampled point rows.
//!
//! Child transitions are a fixed set, resolved by name in `resolve_child`.
//! `build_hierarchy` verifies the cross-format alignment invariants (parent
//! rows == child entities) before exposing the states.

use pyo3::prelude::*;

use super::tbo_collection::{validate_cross_format, CollectionState, FormatPair};
use super::tbo_entity::HierarchicalEntity;

#[derive(Clone, Copy, PartialEq)]
pub enum StateRef {
    Scenes,
    Assets,
    Fragments,
    Points,
    Faces,
    /// Terminal level: sampled points are the rows of their parent fragment
    /// entity, so they have no backing collection state of their own.
    SampledPoints,
}

impl StateRef {
    pub fn get_state<'a>(&self, h: &'a TBOHierarchy) -> Option<&'a CollectionState> {
        match self {
            StateRef::Scenes => Some(&h.scenes_state),
            StateRef::Assets => Some(&h.assets_state),
            StateRef::Fragments => Some(&h.fragments_state),
            StateRef::Points => Some(&h.points_state),
            StateRef::Faces => Some(&h.faces_state),
            StateRef::SampledPoints => None,
        }
    }
}

/// A contiguous range of entities within one state, plus the state it belongs to.
#[derive(Clone, Copy)]
pub struct ChildInfo {
    /// Range start within the child state.
    pub offset: usize,
    pub count: usize,
    pub state: StateRef,
}

#[pyclass(unsendable)]
pub struct TBOHierarchy {
    pub scenes_state: CollectionState,
    pub assets_state: CollectionState,
    pub fragments_state: CollectionState,
    pub points_state: CollectionState,
    pub faces_state: CollectionState,
}

/// Build a hierarchy from the five format pairs, in the order scenes, assets,
/// fragments, points, faces.
pub fn build_hierarchy(
    scenes: FormatPair,
    assets: FormatPair,
    fragments: FormatPair,
    points: FormatPair,
    faces: FormatPair,
) -> PyResult<TBOHierarchy> {
    TBOHierarchy::new(
        CollectionState::build(scenes.0, scenes.1)?,
        CollectionState::build(assets.0, assets.1)?,
        CollectionState::build(fragments.0, fragments.1)?,
        CollectionState::build(points.0, points.1)?,
        CollectionState::build(faces.0, faces.1)?,
    )
}

/// Root cursor over every entity of one state.
fn root(slf: PyRef<TBOHierarchy>, py: Python, state: StateRef) -> HierarchicalEntity {
    let total = state
        .get_state(&slf)
        .expect("root states always have a backing")
        .total_entities();
    let keepalive = unsafe { Py::from_owned_ptr(py, slf.into_ptr()) };
    HierarchicalEntity::new(py, keepalive, ChildInfo { offset: 0, count: total, state })
}

#[pymethods]
impl TBOHierarchy {
    #[getter(Scenes)]
    fn scenes(slf: PyRef<Self>, py: Python) -> HierarchicalEntity {
        root(slf, py, StateRef::Scenes)
    }

    #[getter(Assets)]
    fn assets(slf: PyRef<Self>, py: Python) -> HierarchicalEntity {
        root(slf, py, StateRef::Assets)
    }

    #[getter(Fragments)]
    fn fragments(slf: PyRef<Self>, py: Python) -> HierarchicalEntity {
        root(slf, py, StateRef::Fragments)
    }

    #[getter(Points)]
    fn points(slf: PyRef<Self>, py: Python) -> HierarchicalEntity {
        root(slf, py, StateRef::Points)
    }

    #[getter(Faces)]
    fn faces(slf: PyRef<Self>, py: Python) -> HierarchicalEntity {
        root(slf, py, StateRef::Faces)
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

    #[getter(points_count)]
    fn points_count(&self) -> usize {
        self.points_state.total_entities()
    }

    #[getter(faces_count)]
    fn faces_count(&self) -> usize {
        self.faces_state.total_entities()
    }
}

impl TBOHierarchy {
    pub fn new(
        scenes: CollectionState,
        assets: CollectionState,
        fragments: CollectionState,
        points: CollectionState,
        faces: CollectionState,
    ) -> PyResult<Self> {
        validate_cross_format(&scenes, &assets, &fragments)?;
        // Points and Faces should align with Fragments (same entity count as fragments)
        if !fragments.views.is_empty() && !points.views.is_empty() {
            if fragments.total_entities() != points.total_entities() {
                return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
                    "hierarchy misaligned: {} fragment entit(ies) != {} points entit(ies). \
                     Points and Faces should have the same entity count as Fragments.",
                    fragments.total_entities(),
                    points.total_entities()
                )));
            }
        }
        if !fragments.views.is_empty() && !faces.views.is_empty() {
            if fragments.total_entities() != faces.total_entities() {
                return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
                    "hierarchy misaligned: {} fragment entit(ies) != {} faces entit(ies). \
                     Points and Faces should have the same entity count as Fragments.",
                    fragments.total_entities(),
                    faces.total_entities()
                )));
            }
        }
        Ok(Self {
            scenes_state: scenes,
            assets_state: assets,
            fragments_state: fragments,
            points_state: points,
            faces_state: faces,
        })
    }

    pub fn resolve_child(
        &self,
        name: &str,
        parent_idx: usize,
        caller_state: StateRef,
    ) -> Option<ChildInfo> {
        let (expected_parent, child_state) = match name {
            "Assets" => (StateRef::Scenes, StateRef::Assets),
            "Fragments" => (StateRef::Assets, StateRef::Fragments),
            "Points" => (StateRef::Assets, StateRef::Points),
            "Faces" => (StateRef::Assets, StateRef::Faces),
            // Terminal: the sampled points are the rows of the parent fragment.
            "SampledPoints" => (StateRef::Fragments, StateRef::SampledPoints),
            _ => return None,
        };
        if expected_parent != caller_state {
            return None;
        }
        let parent_state = expected_parent.get_state(self)?;
        let offset = parent_state.cumulative_rows_before(parent_idx).ok()?;
        let count = parent_state.entity_row_count(parent_idx).ok()?;
        if count == 0 {
            return None;
        }
        Some(ChildInfo {
            // SampledPoints are relative to the parent fragment (start at 0).
            offset: if child_state == StateRef::SampledPoints { 0 } else { offset },
            count,
            state: child_state,
        })
    }
}
