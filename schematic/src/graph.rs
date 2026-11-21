//! Graph-based schematic operations.

use crate::netlist::Netlist;
use std::collections::HashSet;

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
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum NodeType {
    #[default]
    Symbol,
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

impl Graph {
    /// Build a connectivity graph: one node per symbol, one node per net.
    ///
    /// Every [`Connection`](crate::netlist::Connection) becomes an edge between
    /// the symbol nodes owning its source and target pins. Connections that
    /// reference a symbol which is not part of the netlist are skipped. Edge ids
    /// are derived from the connection, so building the graph twice from the
    /// same netlist produces the same edges.
    pub fn from_netlist(netlist: &Netlist) -> Graph {
        let mut nodes = Vec::with_capacity(netlist.symbols.len() + netlist.nets.len());
        let mut symbol_ids = HashSet::new();

        for symbol in &netlist.symbols {
            symbol_ids.insert(symbol.id);
            nodes.push(Node {
                id: symbol.id,
                r#type: NodeType::Symbol,
                properties: vec![NodeProperty { name: "name".into(), value: symbol.name.clone() }],
            });
        }

        for net in &netlist.nets {
            nodes.push(Node {
                id: net.id,
                r#type: NodeType::Net,
                properties: vec![NodeProperty { name: "name".into(), value: net.name.clone() }],
            });
        }

        let mut edges = Vec::new();
        for connection in &netlist.connections {
            let source = connection.source_pin.symbol_id;
            let target = connection.target_pin.symbol_id;
            if !symbol_ids.contains(&source) || !symbol_ids.contains(&target) {
                continue;
            }
            edges.push(Edge {
                id: uuid::Uuid::new_v5(
                    &uuid::Uuid::NAMESPACE_OID,
                    format!("edge|{}|{}|{}", connection.wire_id, source, target).as_bytes(),
                ),
                source,
                target,
                properties: vec![EdgeProperty {
                    name: "wire_id".into(),
                    value: connection.wire_id.to_string(),
                }],
            });
        }

        Graph { nodes, edges }
    }

    /// Number of nodes in the graph.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Number of edges in the graph.
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }
}

#[cfg(test)]
mod tests {
    use super::{Graph, NodeType};
    use crate::netlist::{Connection, Netlist, PinRef, generate_netlist};
    use core::net::Net;
    use core::symbol::{Pin, Symbol};
    use uuid::Uuid;

    fn symbol(name: &str, pin_ids: &[Uuid]) -> Symbol {
        Symbol {
            id: Uuid::new_v4(),
            name: name.into(),
            pins: pin_ids
                .iter()
                .enumerate()
                .map(|(i, id)| Pin { id: *id, number: (i + 1).to_string(), ..Pin::default() })
                .collect(),
            library: "test".into(),
        }
    }

    #[test]
    fn from_netlist_counts_nodes_and_edges() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let first = symbol("R1", &[a]);
        let second = symbol("R2", &[b]);
        let net = Net { id: Uuid::new_v4(), name: "N1".into(), pins: vec![a, b], ..Net::default() };
        let netlist =
            generate_netlist(&[first.clone(), second.clone()], std::slice::from_ref(&net));

        let graph = Graph::from_netlist(&netlist);

        assert_eq!(graph.node_count(), 3);
        assert_eq!(graph.edge_count(), 1);

        let symbol_nodes: Vec<_> =
            graph.nodes.iter().filter(|n| n.r#type == NodeType::Symbol).collect();
        assert_eq!(symbol_nodes.len(), 2);

        let net_nodes: Vec<_> = graph.nodes.iter().filter(|n| n.r#type == NodeType::Net).collect();
        assert_eq!(net_nodes.len(), 1);
        assert_eq!(net_nodes[0].id, net.id);

        let edge = &graph.edges[0];
        assert_eq!(edge.source, first.id);
        assert_eq!(edge.target, second.id);
        assert_eq!(edge.properties.len(), 1);
        assert_eq!(edge.properties[0].name, "wire_id");
        assert_eq!(edge.properties[0].value, netlist.connections[0].wire_id.to_string());
    }

    #[test]
    fn empty_netlist_builds_empty_graph() {
        let graph = Graph::from_netlist(&Netlist::default());

        assert_eq!(graph.node_count(), 0);
        assert_eq!(graph.edge_count(), 0);
    }

    #[test]
    fn graph_generation_is_deterministic() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let first = symbol("R1", &[a]);
        let second = symbol("R2", &[b]);
        let net = Net { id: Uuid::new_v4(), name: "N1".into(), pins: vec![a, b], ..Net::default() };
        let netlist = generate_netlist(&[first, second], std::slice::from_ref(&net));

        let one = Graph::from_netlist(&netlist);
        let two = Graph::from_netlist(&netlist);

        assert_eq!(one.edge_count(), 1);
        assert_eq!(one.edges[0].id, two.edges[0].id);
        assert_eq!(one.node_count(), two.node_count());
    }

    #[test]
    fn connections_to_unknown_symbols_are_skipped() {
        let a = Uuid::new_v4();
        let symbol = symbol("R1", &[a]);
        let mut netlist = Netlist::default();
        netlist.symbols.push(symbol.clone());
        netlist.connections.push(Connection {
            source_pin: PinRef { symbol_id: symbol.id, pin_number: "1".into() },
            target_pin: PinRef { symbol_id: Uuid::new_v4(), pin_number: "1".into() },
            wire_id: Uuid::new_v4(),
        });

        let graph = Graph::from_netlist(&netlist);

        assert_eq!(graph.node_count(), 1);
        assert_eq!(graph.edge_count(), 0);
    }
}
