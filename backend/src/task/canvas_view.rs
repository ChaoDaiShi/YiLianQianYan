//! Presentation-only state for the v1 Infinite Task Control Surface.
//!
//! A `CanvasView` is deliberately separate from `TaskGraph`: moving a node,
//! changing the viewport, or selecting a node must never mutate semantic graph
//! state or its revision.  The value is validated at every persistence
//! boundary because serde can construct values without calling constructors.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::{TaskGraph, TaskGraphId, TaskNodeId, MAX_TASK_GRAPH_NODES};

pub const CANVAS_VIEW_SCHEMA_VERSION: u32 = 1;
pub const MAX_CANVAS_VIEW_LAYOUTS: usize = MAX_TASK_GRAPH_NODES;
pub const MAX_CANVAS_VIEW_SELECTION: usize = MAX_TASK_GRAPH_NODES;
pub const MAX_CANVAS_VIEW_COORDINATE: f64 = 1_000_000.0;
pub const MIN_CANVAS_VIEW_ZOOM: f64 = 0.1;
pub const MAX_CANVAS_VIEW_ZOOM: f64 = 4.0;
pub const MIN_CANVAS_NODE_WIDTH: f64 = 80.0;
pub const MAX_CANVAS_NODE_WIDTH: f64 = 800.0;
pub const MIN_CANVAS_NODE_HEIGHT: f64 = 48.0;
pub const MAX_CANVAS_NODE_HEIGHT: f64 = 600.0;
pub const MAX_TASK_REVISION_HISTORY: usize = 32;

const DEFAULT_NODE_WIDTH: f64 = 240.0;
const DEFAULT_NODE_HEIGHT: f64 = 128.0;
const AUTO_LAYOUT_ORIGIN_X: f64 = 40.0;
const AUTO_LAYOUT_ORIGIN_Y: f64 = 40.0;
const AUTO_LAYOUT_COLUMN_GAP: f64 = 320.0;
const AUTO_LAYOUT_ROW_GAP: f64 = 200.0;
const AUTO_LAYOUT_COLUMNS: usize = 4;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CanvasViewport {
    pub x: f64,
    pub y: f64,
    pub zoom: f64,
}

impl Default for CanvasViewport {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            zoom: 1.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CanvasNodeLayout {
    pub node_id: TaskNodeId,
    pub x: f64,
    pub y: f64,
    #[serde(default = "default_node_width")]
    pub width: f64,
    #[serde(default = "default_node_height")]
    pub height: f64,
}

impl CanvasNodeLayout {
    pub fn new(node_id: TaskNodeId, x: f64, y: f64) -> Self {
        Self {
            node_id,
            x,
            y,
            width: DEFAULT_NODE_WIDTH,
            height: DEFAULT_NODE_HEIGHT,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CanvasView {
    pub schema_version: u32,
    pub graph_id: TaskGraphId,
    pub view_revision: u64,
    pub graph_revision_seen: u64,
    pub viewport: CanvasViewport,
    pub node_layouts: Vec<CanvasNodeLayout>,
    pub selection: Vec<TaskNodeId>,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskRevisionSummary {
    pub graph_id: TaskGraphId,
    pub revision: u64,
    pub change: String,
    pub node_count: usize,
    pub edge_count: usize,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskCheckpointSummary {
    pub checkpoint_id: String,
    pub graph_id: TaskGraphId,
    pub graph_revision: u64,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CanvasViewError {
    #[error("unsupported canvas view schema version: {0}")]
    UnsupportedSchemaVersion(u32),
    #[error("canvas view graph identity does not match the graph")]
    GraphIdentityMismatch,
    #[error("canvas view graph is invalid: {0}")]
    InvalidGraph(String),
    #[error("canvas view graph revision must be positive")]
    InvalidGraphRevision,
    #[error("canvas view revision must be positive")]
    InvalidViewRevision,
    #[error("canvas view contains an invalid identifier: {0}")]
    InvalidIdentifier(String),
    #[error("canvas view contains too many node layouts: {0}")]
    TooManyLayouts(usize),
    #[error("canvas view contains too many selected nodes: {0}")]
    TooManySelections(usize),
    #[error("canvas view contains a duplicate node layout: {0}")]
    DuplicateLayout(String),
    #[error("canvas view contains a duplicate selection: {0}")]
    DuplicateSelection(String),
    #[error("canvas view references a node that is not in the graph: {0}")]
    UnknownNode(String),
    #[error("canvas view contains a non-finite {field}")]
    NonFinite { field: &'static str },
    #[error("canvas view {field} is outside its supported bounds")]
    OutOfBounds { field: &'static str },
}

impl CanvasView {
    pub fn initial(graph: &TaskGraph, now: i64) -> Result<Self, CanvasViewError> {
        graph
            .validate()
            .map_err(|error| CanvasViewError::InvalidGraph(error.to_string()))?;
        let node_layouts = graph
            .nodes
            .iter()
            .enumerate()
            .map(|(index, node)| {
                let (x, y) = auto_position(index);
                CanvasNodeLayout::new(node.id.clone(), x, y)
            })
            .collect();
        let view = Self {
            schema_version: CANVAS_VIEW_SCHEMA_VERSION,
            graph_id: graph.id.clone(),
            view_revision: 1,
            graph_revision_seen: graph.revision.value(),
            viewport: CanvasViewport::default(),
            node_layouts,
            selection: Vec::new(),
            updated_at: now,
        };
        view.validate_for_graph(graph)?;
        Ok(view)
    }

    pub fn validate_for_graph(&self, graph: &TaskGraph) -> Result<(), CanvasViewError> {
        graph
            .validate()
            .map_err(|error| CanvasViewError::InvalidGraph(error.to_string()))?;
        self.validate_shape()?;
        if self.graph_id != graph.id {
            return Err(CanvasViewError::GraphIdentityMismatch);
        }
        let graph_nodes: HashSet<&str> = graph.nodes.iter().map(|node| node.id.as_str()).collect();
        for layout in &self.node_layouts {
            if !graph_nodes.contains(layout.node_id.as_str()) {
                return Err(CanvasViewError::UnknownNode(layout.node_id.to_string()));
            }
        }
        for node_id in &self.selection {
            if !graph_nodes.contains(node_id.as_str()) {
                return Err(CanvasViewError::UnknownNode(node_id.to_string()));
            }
        }
        Ok(())
    }

    /// Validate all scalar, identifier, count, and uniqueness constraints
    /// without requiring every layout node to still exist in the graph.  The
    /// latter is intentionally left to `reconcile`, which removes layouts for
    /// nodes deleted after the view was saved.
    pub(crate) fn validate_shape(&self) -> Result<(), CanvasViewError> {
        self.validate_shape_without_graph_nodes()
    }

    /// Reconcile a previously persisted view against the current graph.
    /// Existing locations remain byte-for-byte unchanged.  Removed nodes lose
    /// their layout/selection; new nodes receive the first deterministic free
    /// grid slot, so graph edits never globally rearrange a user's canvas.
    pub fn reconcile(&mut self, graph: &TaskGraph, now: i64) -> Result<(), CanvasViewError> {
        graph
            .validate()
            .map_err(|error| CanvasViewError::InvalidGraph(error.to_string()))?;
        if self.graph_id != graph.id {
            return Err(CanvasViewError::GraphIdentityMismatch);
        }
        self.validate_shape_without_graph_nodes()?;

        let graph_ids: HashSet<&str> = graph.nodes.iter().map(|node| node.id.as_str()).collect();
        let before_layout_count = self.node_layouts.len();
        let before_selection_count = self.selection.len();
        self.node_layouts
            .retain(|layout| graph_ids.contains(layout.node_id.as_str()));
        self.selection
            .retain(|node_id| graph_ids.contains(node_id.as_str()));

        let mut occupied = self
            .node_layouts
            .iter()
            .map(|layout| (layout.x, layout.y))
            .collect::<Vec<_>>();
        for (index, node) in graph.nodes.iter().enumerate() {
            if self
                .node_layouts
                .iter()
                .any(|layout| layout.node_id == node.id)
            {
                continue;
            }
            let (x, y) = first_free_position(index, &occupied);
            let layout = CanvasNodeLayout::new(node.id.clone(), x, y);
            self.node_layouts.push(layout);
            occupied.push((x, y));
        }
        let changed = before_layout_count != self.node_layouts.len()
            || before_selection_count != self.selection.len()
            || self.graph_revision_seen != graph.revision.value();
        self.graph_revision_seen = graph.revision.value();
        if changed {
            self.updated_at = now;
        }
        self.validate_for_graph(graph)
    }

    fn validate_shape_without_graph_nodes(&self) -> Result<(), CanvasViewError> {
        if self.schema_version != CANVAS_VIEW_SCHEMA_VERSION {
            return Err(CanvasViewError::UnsupportedSchemaVersion(
                self.schema_version,
            ));
        }
        if !valid_identifier(self.graph_id.as_str()) {
            return Err(CanvasViewError::InvalidIdentifier(
                self.graph_id.to_string(),
            ));
        }
        if self.view_revision == 0 {
            return Err(CanvasViewError::InvalidViewRevision);
        }
        if self.graph_revision_seen == 0 {
            return Err(CanvasViewError::InvalidGraphRevision);
        }
        if self.node_layouts.len() > MAX_CANVAS_VIEW_LAYOUTS {
            return Err(CanvasViewError::TooManyLayouts(self.node_layouts.len()));
        }
        if self.selection.len() > MAX_CANVAS_VIEW_SELECTION {
            return Err(CanvasViewError::TooManySelections(self.selection.len()));
        }
        validate_viewport(&self.viewport)?;
        let mut layouts = HashSet::with_capacity(self.node_layouts.len());
        for layout in &self.node_layouts {
            validate_node_id(&layout.node_id)?;
            if !layouts.insert(layout.node_id.as_str()) {
                return Err(CanvasViewError::DuplicateLayout(layout.node_id.to_string()));
            }
            validate_coordinate(layout.x, "node layout x")?;
            validate_coordinate(layout.y, "node layout y")?;
            validate_size(
                layout.width,
                MIN_CANVAS_NODE_WIDTH,
                MAX_CANVAS_NODE_WIDTH,
                "node layout width",
            )?;
            validate_size(
                layout.height,
                MIN_CANVAS_NODE_HEIGHT,
                MAX_CANVAS_NODE_HEIGHT,
                "node layout height",
            )?;
        }
        let mut selection = HashSet::with_capacity(self.selection.len());
        for node_id in &self.selection {
            validate_node_id(node_id)?;
            if !selection.insert(node_id.as_str()) {
                return Err(CanvasViewError::DuplicateSelection(node_id.to_string()));
            }
        }
        Ok(())
    }
}

fn default_node_width() -> f64 {
    DEFAULT_NODE_WIDTH
}

fn default_node_height() -> f64 {
    DEFAULT_NODE_HEIGHT
}

fn validate_viewport(viewport: &CanvasViewport) -> Result<(), CanvasViewError> {
    validate_coordinate(viewport.x, "viewport x")?;
    validate_coordinate(viewport.y, "viewport y")?;
    if !viewport.zoom.is_finite() {
        return Err(CanvasViewError::NonFinite {
            field: "viewport zoom",
        });
    }
    if !(MIN_CANVAS_VIEW_ZOOM..=MAX_CANVAS_VIEW_ZOOM).contains(&viewport.zoom) {
        return Err(CanvasViewError::OutOfBounds {
            field: "viewport zoom",
        });
    }
    Ok(())
}

fn validate_coordinate(value: f64, field: &'static str) -> Result<(), CanvasViewError> {
    if !value.is_finite() {
        return Err(CanvasViewError::NonFinite { field });
    }
    if value.abs() > MAX_CANVAS_VIEW_COORDINATE {
        return Err(CanvasViewError::OutOfBounds { field });
    }
    Ok(())
}

fn validate_size(
    value: f64,
    minimum: f64,
    maximum: f64,
    field: &'static str,
) -> Result<(), CanvasViewError> {
    if !value.is_finite() {
        return Err(CanvasViewError::NonFinite { field });
    }
    if !(minimum..=maximum).contains(&value) {
        return Err(CanvasViewError::OutOfBounds { field });
    }
    Ok(())
}

fn validate_node_id(node_id: &TaskNodeId) -> Result<(), CanvasViewError> {
    if !valid_identifier(node_id.as_str()) {
        return Err(CanvasViewError::InvalidIdentifier(node_id.to_string()));
    }
    Ok(())
}

fn valid_identifier(raw: &str) -> bool {
    !raw.is_empty()
        && raw.chars().count() <= 64
        && raw.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.')
        })
}

fn auto_position(index: usize) -> (f64, f64) {
    (
        AUTO_LAYOUT_ORIGIN_X + (index % AUTO_LAYOUT_COLUMNS) as f64 * AUTO_LAYOUT_COLUMN_GAP,
        AUTO_LAYOUT_ORIGIN_Y + (index / AUTO_LAYOUT_COLUMNS) as f64 * AUTO_LAYOUT_ROW_GAP,
    )
}

fn first_free_position(index: usize, occupied: &[(f64, f64)]) -> (f64, f64) {
    let mut candidate_index = index;
    loop {
        let candidate = auto_position(candidate_index);
        if occupied
            .iter()
            .all(|(x, y)| *x != candidate.0 || *y != candidate.1)
        {
            return candidate;
        }
        candidate_index = candidate_index.saturating_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn graph(revision: u64, ids: &[&str]) -> TaskGraph {
        let nodes = ids
            .iter()
            .map(|id| {
                super::super::TaskNode::new(
                    TaskNodeId::new(*id).unwrap(),
                    super::super::TaskNodeKind::Work,
                    format!("Task {id}"),
                    json!({}),
                )
                .unwrap()
            })
            .collect();
        TaskGraph::new(
            TaskGraphId::new("canvas").unwrap(),
            super::super::GraphRevision::new(revision).unwrap(),
            nodes,
            Vec::new(),
        )
        .unwrap()
    }

    #[test]
    fn initial_view_is_deterministic_and_reconcile_preserves_manual_locations() {
        let original = graph(1, &["a", "b"]);
        let mut view = CanvasView::initial(&original, 100).unwrap();
        view.node_layouts[0].x = 777.0;

        let changed = graph(2, &["a", "c"]);
        view.reconcile(&changed, 200).unwrap();

        assert_eq!(view.view_revision, 1);
        assert_eq!(view.graph_revision_seen, 2);
        assert_eq!(view.node_layouts[0].x, 777.0);
        assert_eq!(view.node_layouts.len(), 2);
        assert!(view
            .node_layouts
            .iter()
            .any(|layout| layout.node_id.as_str() == "c"));
        assert!(view
            .node_layouts
            .iter()
            .all(|layout| layout.node_id.as_str() != "b"));
    }

    #[test]
    fn validation_rejects_non_finite_and_unbounded_values() {
        let graph = graph(1, &["a"]);
        let mut view = CanvasView::initial(&graph, 100).unwrap();
        view.viewport.zoom = f64::NAN;
        assert!(view.validate_for_graph(&graph).is_err());

        let mut view = CanvasView::initial(&graph, 100).unwrap();
        view.viewport.zoom = 99.0;
        assert!(view.validate_for_graph(&graph).is_err());

        let mut view = CanvasView::initial(&graph, 100).unwrap();
        view.node_layouts[0].width = 10_000.0;
        assert!(view.validate_for_graph(&graph).is_err());
    }
}
