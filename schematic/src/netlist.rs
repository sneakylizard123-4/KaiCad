//! Netlist generation and management.

use core::net::Net;
use core::symbol::Symbol;
use std::collections::HashMap;
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

/// Escape a string for use inside a quoted S-expression token.
///
/// Backslashes and quotes are escaped, and line breaks or tabs are replaced by
/// their escaped forms so a quoted value never contains a raw newline.
fn escape_token(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            other => escaped.push(other),
        }
    }
    escaped
}

/// Render the netlist as a KiCad-compatible S-expression netlist document.
///
/// This is export only: the [`Netlist`] remains KaiCad's internal
/// representation and nothing here is parsed back into it.
///
/// - Components come from [`Netlist::symbols`], sorted by `(name, id)`. The
///   `value` and `footprint` fields are empty because a netlist carries no
///   component properties yet.
/// - Nets come from [`Netlist::nets`], sorted by `(name, id)`, and are assigned
///   codes starting at `1` in emission order.
/// - Each net lists the pins in [`Net::pins`](core::net::Net::pins) that resolve
///   to a known symbol pin, sorted by `(reference, pin number, pin id)`. A net
///   with no resolvable node is still emitted, without nodes.
/// - Quotes, backslashes and line breaks in names and pin numbers are escaped.
///
/// The output is deterministic, pretty-printed with two-space indentation, uses
/// `\n` line endings and ends with exactly one trailing newline.
pub fn to_kicad_netlist(netlist: &Netlist) -> String {
    let mut symbols: Vec<&Symbol> = netlist.symbols.iter().collect();
    symbols.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));

    let mut nets: Vec<&Net> = netlist.nets.iter().collect();
    nets.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));

    let mut pin_owners: HashMap<Uuid, (Uuid, &str)> = HashMap::new();
    for symbol in &netlist.symbols {
        for pin in &symbol.pins {
            pin_owners.insert(pin.id, (symbol.id, pin.number.as_str()));
        }
    }
    let symbol_names: HashMap<Uuid, &str> =
        netlist.symbols.iter().map(|symbol| (symbol.id, symbol.name.as_str())).collect();

    let mut document = String::new();
    document.push_str("(export (version \"E\")\n");

    if symbols.is_empty() {
        document.push_str("  (components)\n");
    } else {
        document.push_str("  (components\n");
        for symbol in &symbols {
            document.push_str(&format!(
                "    (comp (ref \"{}\") (value \"\") (footprint \"\"))\n",
                escape_token(&symbol.name)
            ));
        }
        document.push_str("  )\n");
    }

    if nets.is_empty() {
        document.push_str("  (nets)\n");
    } else {
        document.push_str("  (nets\n");
        for (index, net) in nets.iter().enumerate() {
            document.push_str(&format!(
                "    (net (code \"{}\") (name \"{}\")\n",
                index + 1,
                escape_token(&net.name)
            ));

            let mut nodes: Vec<(&str, &str, Uuid)> = net
                .pins
                .iter()
                .filter_map(|pin_id| {
                    let (symbol_id, number) = pin_owners.get(pin_id)?;
                    let reference = symbol_names.get(symbol_id)?;
                    Some((*reference, *number, *pin_id))
                })
                .collect();
            nodes.sort_unstable();

            for (reference, number, _) in nodes {
                document.push_str(&format!(
                    "      (node (ref \"{}\") (pin \"{}\"))\n",
                    escape_token(reference),
                    escape_token(number)
                ));
            }
            document.push_str("    )\n");
        }
        document.push_str("  )\n");
    }

    document.push_str(")\n");
    document
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
    use super::{Netlist, PinRef, generate_netlist, to_kicad_netlist};
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

    fn symbol_named(name: &str, pins: &[(Uuid, &str)]) -> Symbol {
        Symbol {
            id: Uuid::new_v4(),
            name: name.into(),
            pins: pins
                .iter()
                .map(|(id, number)| Pin {
                    id: *id,
                    number: (*number).to_string(),
                    ..Pin::default()
                })
                .collect(),
            library: "test".into(),
        }
    }

    /// Two components joined by two nets: N1 links pin 1s, N2 links pin 2s.
    fn two_symbol_fixture(
        component_names: [&str; 2],
        net_names: [&str; 2],
    ) -> (Vec<Symbol>, Vec<Net>) {
        let (a1, a2, b1, b2) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
        let first = symbol_named(component_names[0], &[(a1, "1"), (a2, "2")]);
        let second = symbol_named(component_names[1], &[(b1, "1"), (b2, "2")]);
        let first_net = Net {
            id: Uuid::new_v4(),
            name: net_names[0].into(),
            pins: vec![a1, b1],
            ..Net::default()
        };
        let second_net = Net {
            id: Uuid::new_v4(),
            name: net_names[1].into(),
            pins: vec![a2, b2],
            ..Net::default()
        };
        (vec![first, second], vec![first_net, second_net])
    }

    #[test]
    fn kicad_export_contains_components_and_nets() {
        let (symbols, nets) = two_symbol_fixture(["R1", "R2"], ["N1", "N2"]);

        let document = to_kicad_netlist(&generate_netlist(&symbols, &nets));

        assert!(document.starts_with("(export (version \"E\")\n"));
        assert!(document.contains("    (comp (ref \"R1\") (value \"\") (footprint \"\"))"));
        assert!(document.contains("    (comp (ref \"R2\") (value \"\") (footprint \"\"))"));
        assert!(document.contains("    (net (code \"1\") (name \"N1\")"));
        assert!(document.contains("    (net (code \"2\") (name \"N2\")"));
        assert!(document.contains("      (node (ref \"R1\") (pin \"1\"))"));
        assert!(document.contains("      (node (ref \"R2\") (pin \"2\"))"));
    }

    #[test]
    fn kicad_export_is_byte_identical_across_calls() {
        let (symbols, nets) = two_symbol_fixture(["R1", "R2"], ["N1", "N2"]);
        let netlist = generate_netlist(&symbols, &nets);

        let first = to_kicad_netlist(&netlist);
        let second = to_kicad_netlist(&netlist);
        let rebuilt = to_kicad_netlist(&generate_netlist(&symbols, &nets));

        assert_eq!(first, second);
        assert_eq!(first, rebuilt);
    }

    #[test]
    fn kicad_export_sorts_components_and_nets() {
        let (symbols, nets) = two_symbol_fixture(["R2", "R1"], ["NB", "NA"]);

        let document = to_kicad_netlist(&generate_netlist(&symbols, &nets));

        let first_component = document.find("(ref \"R1\")").expect("R1 is exported");
        let second_component = document.find("(ref \"R2\")").expect("R2 is exported");
        assert!(first_component < second_component);

        // Codes are assigned in the sorted order, not the input order.
        assert!(document.contains("    (net (code \"1\") (name \"NA\")"));
        assert!(document.contains("    (net (code \"2\") (name \"NB\")"));
    }

    #[test]
    fn kicad_export_escapes_quotes_and_backslashes() {
        let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
        let symbol = symbol_named("R\"1\\2", &[(a, "1"), (b, "2")]);
        let net =
            Net { id: Uuid::new_v4(), name: "N\"1\\2".into(), pins: vec![a, b], ..Net::default() };

        let document = to_kicad_netlist(&generate_netlist(&[symbol], &[net]));

        assert!(document.contains("(ref \"R\\\"1\\\\2\")"));
        assert!(document.contains("(name \"N\\\"1\\\\2\")"));
    }

    #[test]
    fn kicad_export_ends_with_exactly_one_newline() {
        let (symbols, nets) = two_symbol_fixture(["R1", "R2"], ["N1", "N2"]);

        let document = to_kicad_netlist(&generate_netlist(&symbols, &nets));

        assert!(document.ends_with(")\n"));
        assert!(!document.ends_with("\n\n"));
        assert!(!document.contains("\r"));
    }

    #[test]
    fn kicad_export_keeps_nets_without_resolvable_nodes() {
        let symbol = symbol_named("R1", &[(Uuid::new_v4(), "1")]);
        let net = Net {
            id: Uuid::new_v4(),
            name: "ORPHAN".into(),
            pins: vec![Uuid::new_v4()],
            ..Net::default()
        };

        let document = to_kicad_netlist(&generate_netlist(&[symbol], &[net]));

        assert!(document.contains("(name \"ORPHAN\")"));
        assert!(!document.contains("(node "));
    }

    #[test]
    fn kicad_export_of_empty_netlist_has_empty_sections() {
        let document = to_kicad_netlist(&Netlist::default());

        assert!(document.contains("  (components)\n"));
        assert!(document.contains("  (nets)\n"));
        assert!(document.ends_with(")\n"));
    }
}
