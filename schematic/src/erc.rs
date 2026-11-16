//! Electrical rule checking for schematics.

use crate::netlist::Netlist;

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
}
