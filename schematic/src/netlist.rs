//! Netlist generation and management.

use core::net::Net;
use core::symbol::Symbol;
use uuid::Uuid;

/// Derive the id of a generated netlist from the design content it describes.
///
/// Netlist generation is a deterministic engineering operation: identical
/// symbols and nets must produce an identical document, ids included.
fn netlist_id(symbols: &[Symbol], nets: &[Net]) -> Uuid {
    let mut seed = String::from("netlist");
    for symbol in symbols {
        seed.push_str(&format!("|symbol:{}:{}", symbol.id, symbol.name));
        for pin in &symbol.pins {
            seed.push_str(&format!("|pin:{}:{}", pin.id, pin.number));
        }
    }
    for net in nets {
        seed.push_str(&format!("|net:{}:{}", net.id, net.name));
        for pin in &net.pins {
            seed.push_str(&format!("|pin:{pin}"));
        }
    }
    Uuid::new_v5(&Uuid::NAMESPACE_OID, seed.as_bytes())
}

/// Derive the id of one generated connection.
fn connection_id(net: &Net, index: usize, source: Uuid, target: Uuid) -> Uuid {
    let seed = format!("connection|{}|{index}|{source}|{target}", net.id);
    Uuid::new_v5(&Uuid::NAMESPACE_OID, seed.as_bytes())
}

/// A complete netlist extracted from a schematic.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Netlist {
    pub id: Uuid,
    pub nets: Vec<Net>,
    pub symbols: Vec<Symbol>,
    pub connections: Vec<Connection>,
}

impl Default for Netlist {
    fn default() -> Self {
        Self { id: Uuid::new_v4(), nets: Vec::new(), symbols: Vec::new(), connections: Vec::new() }
    }
}

/// A single connection between two pins in the schematic.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Connection {
    pub source_pin: PinRef,
    pub target_pin: PinRef,
    pub wire_id: Uuid,
}

/// Reference to a pin on a symbol instance.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PinRef {
    pub symbol_id: Uuid,
    pub pin_number: String,
}

/// Build a netlist from schematic symbols and nets.
///
/// Every pin of every symbol is indexed by its [`Pin::id`](core::symbol::Pin::id).
/// Each net's pins are then walked in order and every adjacent pair of pins that
/// both resolve to a known symbol pin becomes a [`Connection`]. Pin ids that do
/// not resolve are skipped.
///
/// Generation is deterministic: the same symbols and nets yield an identical
/// netlist, including the derived [`Netlist::id`] and [`Connection::wire_id`]
/// values.
pub fn generate_netlist(symbols: &[Symbol], nets: &[Net]) -> Netlist {
    let mut pin_refs = std::collections::HashMap::new();
    for symbol in symbols {
        for pin in &symbol.pins {
            pin_refs
                .insert(pin.id, PinRef { symbol_id: symbol.id, pin_number: pin.number.clone() });
        }
    }

    let mut connections = Vec::new();
    for net in nets {
        for (index, pair) in net.pins.windows(2).enumerate() {
            let (Some(source_pin), Some(target_pin)) =
                (pin_refs.get(&pair[0]), pin_refs.get(&pair[1]))
            else {
                continue;
            };
            connections.push(Connection {
                source_pin: source_pin.clone(),
                target_pin: target_pin.clone(),
                wire_id: connection_id(net, index, pair[0], pair[1]),
            });
        }
    }

    Netlist {
        id: netlist_id(symbols, nets),
        nets: nets.to_vec(),
        symbols: symbols.to_vec(),
        connections,
    }
}

#[cfg(test)]
mod tests {
    use super::{PinRef, generate_netlist};
    use core::net::Net;
    use core::symbol::{Pin, Symbol};
    use uuid::Uuid;

    fn symbol_with_pins(pin_ids: &[Uuid]) -> Symbol {
        Symbol {
            id: Uuid::new_v4(),
            name: "R1".into(),
            pins: pin_ids
                .iter()
                .enumerate()
                .map(|(i, id)| Pin { id: *id, number: (i + 1).to_string(), ..Pin::default() })
                .collect(),
            library: "test".into(),
        }
    }

    #[test]
    fn default_netlist_is_empty_with_fresh_id() {
        let netlist = super::Netlist::default();

        assert!(!netlist.id.is_nil());
        assert!(netlist.nets.is_empty());
        assert!(netlist.symbols.is_empty());
        assert!(netlist.connections.is_empty());
    }

    #[test]
    fn two_pin_net_produces_one_connection() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let symbol = symbol_with_pins(&[a, b]);
        let net = Net { id: Uuid::new_v4(), name: "N1".into(), pins: vec![a, b], ..Net::default() };

        let netlist = generate_netlist(std::slice::from_ref(&symbol), std::slice::from_ref(&net));

        assert_eq!(netlist.nets.len(), 1);
        assert_eq!(netlist.symbols.len(), 1);
        assert_eq!(netlist.connections.len(), 1);

        let connection = &netlist.connections[0];
        assert_eq!(connection.source_pin, PinRef { symbol_id: symbol.id, pin_number: "1".into() });
        assert_eq!(connection.target_pin, PinRef { symbol_id: symbol.id, pin_number: "2".into() });
        assert!(!connection.wire_id.is_nil());
    }

    #[test]
    fn unresolvable_pin_ids_are_skipped() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let symbol = symbol_with_pins(&[a, b]);
        let unknown = Uuid::new_v4();
        let net = Net {
            id: Uuid::new_v4(),
            name: "N1".into(),
            pins: vec![a, unknown, b],
            ..Net::default()
        };

        let netlist = generate_netlist(&[symbol], &[net]);

        // [a, unknown] and [unknown, b] both contain an unknown pin.
        assert!(netlist.connections.is_empty());
    }

    #[test]
    fn adjacent_resolvable_pairs_each_produce_a_connection() {
        let ids: Vec<Uuid> = (0..3).map(|_| Uuid::new_v4()).collect();
        let symbol = symbol_with_pins(&ids);
        let net =
            Net { id: Uuid::new_v4(), name: "N1".into(), pins: ids.clone(), ..Net::default() };

        let netlist = generate_netlist(&[symbol], &[net]);

        assert_eq!(netlist.connections.len(), 2);
        assert_eq!(netlist.connections[0].source_pin.pin_number, "1");
        assert_eq!(netlist.connections[0].target_pin.pin_number, "2");
        assert_eq!(netlist.connections[1].source_pin.pin_number, "2");
        assert_eq!(netlist.connections[1].target_pin.pin_number, "3");
    }

    #[test]
    fn generation_is_deterministic() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let symbol = symbol_with_pins(&[a, b]);
        let net = Net { id: Uuid::new_v4(), name: "N1".into(), pins: vec![a, b], ..Net::default() };

        let first = generate_netlist(std::slice::from_ref(&symbol), std::slice::from_ref(&net));
        let second = generate_netlist(std::slice::from_ref(&symbol), std::slice::from_ref(&net));

        assert_eq!(first.id, second.id);
        assert_eq!(first.connections.len(), second.connections.len());
        assert_eq!(first.connections[0].wire_id, second.connections[0].wire_id);
    }

    #[test]
    fn distinct_connections_get_distinct_ids() {
        let ids: Vec<Uuid> = (0..3).map(|_| Uuid::new_v4()).collect();
        let symbol = symbol_with_pins(&ids);
        let net =
            Net { id: Uuid::new_v4(), name: "N1".into(), pins: ids.clone(), ..Net::default() };

        let netlist = generate_netlist(&[symbol], &[net]);

        assert_eq!(netlist.connections.len(), 2);
        assert_ne!(netlist.connections[0].wire_id, netlist.connections[1].wire_id);
    }

    #[test]
    fn pins_from_different_symbols_resolve_to_their_owner() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let first = symbol_with_pins(&[a]);
        let second = symbol_with_pins(&[b]);
        let net = Net { id: Uuid::new_v4(), name: "N1".into(), pins: vec![a, b], ..Net::default() };

        let netlist = generate_netlist(&[first.clone(), second.clone()], &[net]);

        assert_eq!(netlist.connections.len(), 1);
        assert_eq!(netlist.connections[0].source_pin.symbol_id, first.id);
        assert_eq!(netlist.connections[0].target_pin.symbol_id, second.id);
    }
}
