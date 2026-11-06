//! Graph-based schematic operations.

use uuid::Uuid;
use serde::{Serialize, Deserialize};

/// Graph representation for schematic connectivity.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Graph {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}

/// A node in the schematic graph (symbol or net).
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Node {
    pub id: uuid::Uuid,
    pub r#type: NodeType,
    pub properties: Vec<NodeProperty>,
}

/// Type of graph node.
#[derive(Debug, Clone, Copy, Default, serde::Serialize, serde::Deserialize)]
pub enum NodeType {
    #[default] Symbol,
    Net,
}

/// Properties of a graph node.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct NodeProperty {
    pub name: String,
    pub value: String,
}

/// An edge connecting two nodes in the graph.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Edge {
    pub id: uuid::Uuid,
    pub source: uuid::Uuid,
    pub target: uuid::Uuid,
    pub properties: Vec<EdgeProperty>,
}

/// Properties of an edge.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct EdgeProperty {
    pub name: String,
    pub value: String,
}