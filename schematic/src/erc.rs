//! Electrical rule checking for schematics.

use crate::netlist::Netlist;
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

/// Electrical rule check result.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ERCResult {
    pub valid: bool,
    pub violations: Vec<Violation>,
}

impl Default for ERCResult {
    fn default() -> Self {
        Self { valid: true, violations: Vec::new() }
    }
}

impl ERCResult {
    /// Whether the checked design has no error or critical violations.
    pub fn is_valid(&self) -> bool {
        self.valid
    }
}

/// A violation of electrical rules.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Violation {
    pub rule_id: String,
    pub description: String,
    pub location: String,
    pub severity: ViolationSeverity,
}

/// Severity of an electrical rule violation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ViolationSeverity {
    #[default]
    Warning,
    Error,
    Critical,
}

/// Run the built-in electrical rule checks.
///
/// The checks are deterministic and independent of iteration order:
///
/// 1. `net-single-pin` — a net with fewer than two pins is a [`ViolationSeverity::Warning`].
/// 2. `duplicate-net-name` — two nets sharing the same non-empty name are an
///    [`ViolationSeverity::Error`].
/// 3. `self-connection` — a connection whose source and target pin are the same is an
///    [`ViolationSeverity::Error`].
/// 4. `unconnected-pin` — a symbol pin whose id appears in no net is a
///    [`ViolationSeverity::Warning`], one violation per orphaned pin.
/// 5. `duplicate-symbol-name` — two or more symbols sharing a non-empty `name`
///    are an [`ViolationSeverity::Error`], one violation per duplicated name,
///    reported against the second symbol carrying it.
///
/// A result is valid when it contains no [`ViolationSeverity::Error`] or
/// [`ViolationSeverity::Critical`] violations.
pub fn run_erc(netlist: &Netlist) -> ERCResult {
    let mut violations = Vec::new();

    for net in &netlist.nets {
        if net.pins.len() < 2 {
            violations.push(Violation {
                rule_id: "net-single-pin".into(),
                description: format!(
                    "net \"{}\" has {} pin(s); at least 2 are required",
                    net.name,
                    net.pins.len()
                ),
                location: net.id.to_string(),
                severity: ViolationSeverity::Warning,
            });
        }
    }

    for (i, net) in netlist.nets.iter().enumerate() {
        if net.name.is_empty() {
            continue;
        }
        for other in netlist.nets.iter().skip(i + 1) {
            if other.name == net.name {
                violations.push(Violation {
                    rule_id: "duplicate-net-name".into(),
                    description: format!("multiple nets are named \"{}\"", net.name),
                    location: net.id.to_string(),
                    severity: ViolationSeverity::Error,
                });
            }
        }
    }

    for connection in &netlist.connections {
        if connection.source_pin == connection.target_pin {
            violations.push(Violation {
                rule_id: "self-connection".into(),
                description: format!(
                    "pin {} on symbol {} is connected to itself",
                    connection.source_pin.pin_number, connection.source_pin.symbol_id
                ),
                location: connection.wire_id.to_string(),
                severity: ViolationSeverity::Error,
            });
        }
    }

    let connected_pins: HashSet<Uuid> =
        netlist.nets.iter().flat_map(|net| net.pins.iter().copied()).collect();
    for symbol in &netlist.symbols {
        for pin in &symbol.pins {
            if connected_pins.contains(&pin.id) {
                continue;
            }
            violations.push(Violation {
                rule_id: "unconnected-pin".into(),
                description: format!(
                    "pin {} of symbol \"{}\" appears in no net",
                    pin.number, symbol.name
                ),
                location: pin.id.to_string(),
                severity: ViolationSeverity::Warning,
            });
        }
    }

    let mut seen_names: HashMap<&str, Uuid> = HashMap::new();
    let mut reported_names: HashSet<&str> = HashSet::new();
    for symbol in &netlist.symbols {
        if symbol.name.is_empty() {
            continue;
        }
        match seen_names.get(symbol.name.as_str()) {
            Some(first_id) => {
                if reported_names.insert(symbol.name.as_str()) {
                    violations.push(Violation {
                        rule_id: "duplicate-symbol-name".into(),
                        description: format!(
                            "multiple symbols are named \"{}\" (first seen as {})",
                            symbol.name, first_id
                        ),
                        location: symbol.id.to_string(),
                        severity: ViolationSeverity::Error,
                    });
                }
            }
            None => {
                seen_names.insert(symbol.name.as_str(), symbol.id);
            }
        }
    }

    let valid = !violations
        .iter()
        .any(|v| matches!(v.severity, ViolationSeverity::Error | ViolationSeverity::Critical));

    ERCResult { valid, violations }
}

#[cfg(test)]
mod tests {
    use super::{ERCResult, ViolationSeverity, run_erc};
    use crate::netlist::generate_netlist;
    use core::net::Net;
    use core::symbol::{Pin, Symbol};
    use uuid::Uuid;

    fn named_symbol(name: &str, pin_ids: &[Uuid]) -> Symbol {
        Symbol { name: name.into(), ..symbol(pin_ids) }
    }

    fn symbol(pin_ids: &[Uuid]) -> Symbol {
        Symbol {
            id: Uuid::new_v4(),
            name: "U1".into(),
            pins: pin_ids
                .iter()
                .enumerate()
                .map(|(i, id)| Pin { id: *id, number: (i + 1).to_string(), ..Pin::default() })
                .collect(),
            library: "test".into(),
        }
    }

    #[test]
    fn default_result_is_valid() {
        let result = ERCResult::default();

        assert!(result.is_valid());
        assert!(result.violations.is_empty());
    }

    #[test]
    fn clean_netlist_is_valid() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let net = Net { id: Uuid::new_v4(), name: "N1".into(), pins: vec![a, b], ..Net::default() };
        let netlist = generate_netlist(&[symbol(&[a, b])], &[net]);

        let result = run_erc(&netlist);

        assert!(result.is_valid());
        assert!(result.violations.is_empty());
    }

    #[test]
    fn single_pin_net_is_a_warning_only() {
        let a = Uuid::new_v4();
        let net = Net { id: Uuid::new_v4(), name: "N1".into(), pins: vec![a], ..Net::default() };
        let netlist = generate_netlist(&[symbol(&[a])], &[net]);

        let result = run_erc(&netlist);

        assert!(result.is_valid());
        assert_eq!(result.violations.len(), 1);
        assert_eq!(result.violations[0].rule_id, "net-single-pin");
        assert_eq!(result.violations[0].severity, ViolationSeverity::Warning);
    }

    #[test]
    fn duplicate_net_names_are_errors() {
        let pins: Vec<Uuid> = (0..4).map(|_| Uuid::new_v4()).collect();
        let nets = [
            Net {
                id: Uuid::new_v4(),
                name: "VCC".into(),
                pins: pins[0..2].to_vec(),
                ..Net::default()
            },
            Net {
                id: Uuid::new_v4(),
                name: "VCC".into(),
                pins: pins[2..4].to_vec(),
                ..Net::default()
            },
        ];
        let netlist = generate_netlist(&[symbol(&pins)], &nets);

        let result = run_erc(&netlist);

        assert!(!result.is_valid());
        assert_eq!(result.violations.len(), 1);
        assert_eq!(result.violations[0].rule_id, "duplicate-net-name");
        assert_eq!(result.violations[0].severity, ViolationSeverity::Error);
    }

    #[test]
    fn empty_net_names_do_not_collide() {
        let pins: Vec<Uuid> = (0..4).map(|_| Uuid::new_v4()).collect();
        let nets = [
            Net {
                id: Uuid::new_v4(),
                name: String::new(),
                pins: pins[0..2].to_vec(),
                ..Net::default()
            },
            Net {
                id: Uuid::new_v4(),
                name: String::new(),
                pins: pins[2..4].to_vec(),
                ..Net::default()
            },
        ];
        let netlist = generate_netlist(&[symbol(&pins)], &nets);

        let result = run_erc(&netlist);

        assert!(result.is_valid());
        assert!(result.violations.is_empty());
    }

    #[test]
    fn self_connection_is_an_error() {
        let a = Uuid::new_v4();
        let net = Net { id: Uuid::new_v4(), name: "N1".into(), pins: vec![a, a], ..Net::default() };
        let netlist = generate_netlist(&[symbol(&[a])], &[net]);

        assert_eq!(netlist.connections.len(), 1);

        let result = run_erc(&netlist);

        assert!(!result.is_valid());
        assert_eq!(result.violations.len(), 1);
        assert_eq!(result.violations[0].rule_id, "self-connection");
        assert_eq!(result.violations[0].severity, ViolationSeverity::Error);
    }

    #[test]
    fn multiple_rules_are_reported_together() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let c = Uuid::new_v4();
        let d = Uuid::new_v4();
        let nets = [
            // Single pin: warning.
            Net { id: Uuid::new_v4(), name: "A".into(), pins: vec![a], ..Net::default() },
            // Duplicate name: error.
            Net { id: Uuid::new_v4(), name: "B".into(), pins: vec![b, c], ..Net::default() },
            Net { id: Uuid::new_v4(), name: "B".into(), pins: vec![c, d], ..Net::default() },
        ];
        let netlist = generate_netlist(&[symbol(&[a, b, c, d])], &nets);

        let result = run_erc(&netlist);

        assert!(!result.is_valid());
        assert_eq!(result.violations.len(), 2);
        assert_eq!(result.violations[0].rule_id, "net-single-pin");
        assert_eq!(result.violations[1].rule_id, "duplicate-net-name");
    }

    #[test]
    fn unconnected_pins_are_reported_once_each_as_warnings() {
        let (a, b, c) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
        let net = Net { id: Uuid::new_v4(), name: "N1".into(), pins: vec![a], ..Net::default() };
        let netlist = generate_netlist(&[symbol(&[a, b, c])], &[net]);

        let result = run_erc(&netlist);

        let unconnected: Vec<_> =
            result.violations.iter().filter(|v| v.rule_id == "unconnected-pin").collect();
        assert_eq!(unconnected.len(), 2);
        assert!(unconnected.iter().all(|v| v.severity == ViolationSeverity::Warning));
        assert!(unconnected.iter().any(|v| v.description.contains("pin 2")));
        assert_eq!(unconnected[0].location, b.to_string());
        assert_eq!(unconnected[1].location, c.to_string());
        assert!(result.is_valid());
    }

    #[test]
    fn fully_connected_schematic_has_no_new_warnings() {
        let pins: Vec<Uuid> = (0..4).map(|_| Uuid::new_v4()).collect();
        let first = named_symbol("R1", &pins[0..2]);
        let second = named_symbol("R2", &pins[2..4]);
        let net =
            Net { id: Uuid::new_v4(), name: "N1".into(), pins: pins.clone(), ..Net::default() };

        let result = run_erc(&generate_netlist(&[first, second], &[net]));

        assert!(result.is_valid());
        assert!(result.violations.is_empty());
    }

    #[test]
    fn duplicate_symbol_names_are_reported_once_per_name() {
        let pins: Vec<Uuid> = (0..6).map(|_| Uuid::new_v4()).collect();
        let first = named_symbol("R1", &pins[0..2]);
        let second = named_symbol("R1", &pins[2..4]);
        let third = named_symbol("R1", &pins[4..6]);
        let net =
            Net { id: Uuid::new_v4(), name: "N1".into(), pins: pins.clone(), ..Net::default() };

        let result = run_erc(&generate_netlist(&[first, second.clone(), third], &[net]));

        assert!(!result.is_valid());
        let duplicates: Vec<_> =
            result.violations.iter().filter(|v| v.rule_id == "duplicate-symbol-name").collect();
        assert_eq!(duplicates.len(), 1);
        assert_eq!(duplicates[0].severity, ViolationSeverity::Error);
        assert_eq!(duplicates[0].location, second.id.to_string());
        assert!(duplicates[0].description.contains("R1"));
        assert!(result.violations.iter().all(|v| v.rule_id != "unconnected-pin"));
    }
}
