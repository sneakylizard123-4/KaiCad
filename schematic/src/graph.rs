//! Graph-based schematic operations.

use crate::netlist::Netlist;
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

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

    /// Edges attached to `node_id`, in graph order.
    ///
    /// An edge is attached when it starts or ends at the node. Unknown ids
    /// produce an empty list.
    pub fn edges_for(&self, node_id: Uuid) -> Vec<&Edge> {
        self.edges.iter().filter(|edge| edge.source == node_id || edge.target == node_id).collect()
    }

    /// Node ids adjacent to `node_id` through at least one edge.
    ///
    /// The result is sorted and deduplicated, never contains `node_id` itself,
    /// and only contains ids that are nodes in this graph. Unknown ids produce
    /// an empty list.
    pub fn neighbours(&self, node_id: Uuid) -> Vec<Uuid> {
        self.adjacency().get(&node_id).cloned().unwrap_or_default()
    }

    /// Whether two node ids are in the same connected component (BFS).
    ///
    /// A node is always connected to itself. Unknown ids are never connected
    /// to anything, including to themselves.
    pub fn is_connected(&self, a: Uuid, b: Uuid) -> bool {
        let adjacency = self.adjacency();
        if !adjacency.contains_key(&a) || !adjacency.contains_key(&b) {
            return false;
        }
        if a == b {
            return true;
        }

        let mut visited = HashSet::from([a]);
        let mut stack = vec![a];
        while let Some(current) = stack.pop() {
            let Some(neighbours) = adjacency.get(&current) else {
                continue;
            };
            for neighbour in neighbours {
                if !visited.insert(*neighbour) {
                    continue;
                }
                if *neighbour == b {
                    return true;
                }
                stack.push(*neighbour);
            }
        }
        false
    }

    /// Connected components as sorted node-id lists, themselves sorted by their
    /// first node id.
    ///
    /// Nodes without edges form their own single-node component. The result is
    /// deterministic for a given graph.
    pub fn connected_components(&self) -> Vec<Vec<Uuid>> {
        let adjacency = self.adjacency();
        let mut ids: Vec<Uuid> = adjacency.keys().copied().collect();
        ids.sort_unstable();

        let mut visited = HashSet::new();
        let mut components = Vec::new();
        for id in ids {
            if !visited.insert(id) {
                continue;
            }
            let mut component = vec![id];
            let mut stack = vec![id];
            while let Some(current) = stack.pop() {
                let Some(neighbours) = adjacency.get(&current) else {
                    continue;
                };
                for neighbour in neighbours {
                    if visited.insert(*neighbour) {
                        component.push(*neighbour);
                        stack.push(*neighbour);
                    }
                }
            }
            component.sort_unstable();
            components.push(component);
        }
        components
    }

    /// Undirected adjacency list restricted to known node ids.
    ///
    /// Self-loops and edges touching unknown nodes are ignored, and every
    /// neighbour list is sorted and deduplicated.
    fn adjacency(&self) -> HashMap<Uuid, Vec<Uuid>> {
        let known: HashSet<Uuid> = self.nodes.iter().map(|node| node.id).collect();
        let mut adjacency: HashMap<Uuid, Vec<Uuid>> =
            known.iter().map(|id| (*id, Vec::new())).collect();

        for edge in &self.edges {
            if edge.source == edge.target
                || !known.contains(&edge.source)
                || !known.contains(&edge.target)
            {
                continue;
            }
            adjacency.entry(edge.source).or_default().push(edge.target);
            adjacency.entry(edge.target).or_default().push(edge.source);
        }

        for neighbours in adjacency.values_mut() {
            neighbours.sort_unstable();
            neighbours.dedup();
        }
        adjacency
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

    #[test]
    fn edges_for_returns_attached_edges_only() {
        let (a, b, c) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
        let first = symbol("R1", &[a]);
        let second = symbol("R2", &[b]);
        let isolated = symbol("R3", &[c]);
        let net = Net { id: Uuid::new_v4(), name: "N1".into(), pins: vec![a, b], ..Net::default() };
        let graph = Graph::from_netlist(&generate_netlist(
            &[first.clone(), second, isolated.clone()],
            &[net],
        ));

        let edges = graph.edges_for(first.id);
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].source, first.id);

        assert!(graph.edges_for(isolated.id).is_empty());
        assert!(graph.edges_for(Uuid::new_v4()).is_empty());
    }

    #[test]
    fn neighbours_are_distinct_and_exclude_the_node_itself() {
        let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
        let first = symbol("R1", &[a]);
        let second = symbol("R2", &[b]);
        // Two adjacent pin pairs create two edges between the same symbols.
        let net =
            Net { id: Uuid::new_v4(), name: "N1".into(), pins: vec![a, b, a], ..Net::default() };
        let graph =
            Graph::from_netlist(&generate_netlist(&[first.clone(), second.clone()], &[net]));

        assert_eq!(graph.edges_for(first.id).len(), 2);
        assert_eq!(graph.neighbours(first.id), vec![second.id]);
        assert_eq!(graph.neighbours(second.id), vec![first.id]);
        assert!(graph.neighbours(Uuid::new_v4()).is_empty());
    }

    #[test]
    fn is_connected_walks_transitively_and_handles_unknown_ids() {
        let pins: Vec<Uuid> = (0..5).map(|_| Uuid::new_v4()).collect();
        let first = symbol("R1", &[pins[0]]);
        let second = symbol("R2", &[pins[1], pins[2]]);
        let third = symbol("R3", &[pins[3]]);
        let isolated = symbol("R4", &[pins[4]]);
        let first_net = Net {
            id: Uuid::new_v4(),
            name: "N1".into(),
            pins: vec![pins[0], pins[1]],
            ..Net::default()
        };
        let second_net = Net {
            id: Uuid::new_v4(),
            name: "N2".into(),
            pins: vec![pins[2], pins[3]],
            ..Net::default()
        };
        let graph = Graph::from_netlist(&generate_netlist(
            &[first.clone(), second, third.clone(), isolated.clone()],
            &[first_net, second_net],
        ));

        assert!(graph.is_connected(first.id, third.id));
        assert!(graph.is_connected(first.id, first.id));
        assert!(!graph.is_connected(first.id, isolated.id));
        assert!(!graph.is_connected(first.id, Uuid::new_v4()));

        let unknown = Uuid::new_v4();
        assert!(!graph.is_connected(unknown, unknown));
    }

    #[test]
    fn connected_components_are_sorted_and_grouped() {
        let (a, b, c) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
        let first = symbol("R1", &[a]);
        let second = symbol("R2", &[b]);
        let isolated = symbol("R3", &[c]);
        let net = Net { id: Uuid::new_v4(), name: "N1".into(), pins: vec![a, b], ..Net::default() };
        let graph = Graph::from_netlist(&generate_netlist(
            &[first.clone(), second.clone(), isolated.clone()],
            &[net],
        ));

        let components = graph.connected_components();

        // The joined symbols, the isolated symbol and the net node.
        assert_eq!(components.len(), 3);
        for component in &components {
            assert!(component.windows(2).all(|pair| pair[0] < pair[1]));
        }
        assert!(components.windows(2).all(|pair| pair[0][0] < pair[1][0]));

        let joined = components.iter().find(|component| component.contains(&first.id));
        let mut expected = vec![first.id, second.id];
        expected.sort_unstable();
        assert_eq!(joined, Some(&expected));

        let single: Vec<_> =
            components.iter().filter(|component| component.contains(&isolated.id)).collect();
        assert_eq!(single.len(), 1);
        assert_eq!(single[0], &vec![isolated.id]);
    }

    #[test]
    fn empty_graph_has_no_components_edges_or_neighbours() {
        let graph = Graph::default();
        let id = Uuid::new_v4();

        assert!(graph.connected_components().is_empty());
        assert!(graph.neighbours(id).is_empty());
        assert!(graph.edges_for(id).is_empty());
        assert!(!graph.is_connected(id, id));
    }
}
