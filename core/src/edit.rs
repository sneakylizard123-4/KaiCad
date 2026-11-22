//! Atomic, reversible edits to a project.
//!
//! [`apply`] validates every [`ChangeProposal`] against a clone of the project
//! and only replaces the caller's project after all changes succeed. The
//! returned [`EditTransaction`] records the old and new value of every change,
//! so [`undo`] and [`redo`] can restore either side without touching unrelated
//! data.
//!
//! Supported targets are `project` and `component:<reference_designator>`.
//! Component fields are `value` (the property named `Value`),
//! `property:<name>`, and `reference_designator`. Project fields are `name`,
//! `version`, `author` and `description`. Text fields use JSON strings.

use std::collections::HashMap;
use std::fmt;

use serde_json::Value;
use uuid::Uuid;

use crate::{Component, ComponentProperty, Project, proposal::ChangeProposal};

/// One committed edit, with everything needed to undo and redo it.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct EditTransaction {
    /// Stable UUIDv5 id derived from the label and recorded changes.
    pub id: Uuid,
    pub label: String,
    pub changes: Vec<AppliedChange>,
}

/// A single field change recorded by an [`EditTransaction`].
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AppliedChange {
    pub target: String,
    pub field: String,
    pub old_value: Value,
    pub new_value: Value,
    /// Component id when the target is a component. This keeps undo correct
    /// even when the transaction renames the component's reference designator.
    #[serde(default)]
    pub object_id: Option<Uuid>,
}

/// Why an edit could not be applied, undone or redone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditError {
    /// The target string does not name a project or component.
    UnknownTarget(String),
    /// The target does not have the requested field.
    UnknownField { target: String, field: String },
    /// The stored `old_value` no longer matches the project.
    StaleChange { target: String, field: String },
    /// The value cannot be represented by the field.
    InvalidValue { target: String, field: String, message: String },
}

impl fmt::Display for EditError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownTarget(target) => write!(formatter, "unknown edit target `{target}`"),
            Self::UnknownField { target, field } => {
                write!(formatter, "unknown field `{field}` on `{target}`")
            }
            Self::StaleChange { target, field } => {
                write!(formatter, "change to `{field}` on `{target}` is stale")
            }
            Self::InvalidValue { target, field, message } => {
                write!(formatter, "invalid `{field}` on `{target}`: {message}")
            }
        }
    }
}

impl std::error::Error for EditError {}

/// Apply `changes` atomically and return an undoable transaction.
///
/// Every change is validated against a clone of `project`; if any change fails,
/// `project` is left untouched and the first error is returned.
pub fn apply(
    project: &mut Project,
    label: &str,
    changes: &[ChangeProposal],
) -> Result<EditTransaction, EditError> {
    let mut staged = project.clone();
    let mut applied = Vec::with_capacity(changes.len());
    let mut aliases = HashMap::new();
    for change in changes {
        applied.push(apply_change(&mut staged, change, &mut aliases)?);
    }

    let transaction = EditTransaction {
        id: transaction_id(label, &applied),
        label: label.to_string(),
        changes: applied,
    };
    *project = staged;
    Ok(transaction)
}

/// Undo every change in `transaction`, in reverse order.
///
/// The operation is atomic: a stale or malformed transaction leaves `project`
/// unchanged.
pub fn undo(project: &mut Project, transaction: &EditTransaction) -> Result<(), EditError> {
    let mut staged = project.clone();
    for change in transaction.changes.iter().rev() {
        restore_change(&mut staged, change, true)?;
    }
    *project = staged;
    Ok(())
}

/// Redo every change in `transaction`, in its original order.
///
/// The operation is atomic: a stale or malformed transaction leaves `project`
/// unchanged.
pub fn redo(project: &mut Project, transaction: &EditTransaction) -> Result<(), EditError> {
    let mut staged = project.clone();
    for change in &transaction.changes {
        restore_change(&mut staged, change, false)?;
    }
    *project = staged;
    Ok(())
}

fn transaction_id(label: &str, changes: &[AppliedChange]) -> Uuid {
    let mut name = format!("edit:{label}");
    for change in changes {
        name.push_str(&format!(
            "|{}|{}|{}|{}",
            change.target, change.field, change.old_value, change.new_value
        ));
    }
    Uuid::new_v5(&Uuid::NAMESPACE_OID, name.as_bytes())
}

fn apply_change(
    project: &mut Project,
    change: &ChangeProposal,
    aliases: &mut HashMap<String, Uuid>,
) -> Result<AppliedChange, EditError> {
    if change.target == "project" {
        let current = project_value(project, &change.field)?;
        ensure_old(change, &current)?;
        let new = new_string(change)?;
        set_project_value(project, &change.field, new);
        return Ok(AppliedChange {
            target: change.target.clone(),
            field: change.field.clone(),
            old_value: change.old_value.clone(),
            new_value: change.new_value.clone(),
            object_id: None,
        });
    }

    let index = if let Some(id) = aliases.get(&change.target) {
        project
            .schematic
            .components
            .iter()
            .position(|component| component.id == *id)
            .ok_or_else(|| EditError::UnknownTarget(change.target.clone()))?
    } else {
        let reference = component_reference(&change.target)?;
        project
            .schematic
            .components
            .iter()
            .position(|component| component.reference_designator == reference)
            .ok_or_else(|| EditError::UnknownTarget(change.target.clone()))?
    };
    let object_id = project.schematic.components[index].id;
    aliases.entry(change.target.clone()).or_insert(object_id);
    let current =
        component_value(&project.schematic.components[index], &change.target, &change.field)?;
    ensure_old(change, &current)?;

    let component = &mut project.schematic.components[index];
    if change.field == "reference_designator" {
        let new = new_string(change)?;
        if new.is_empty() {
            return Err(invalid(change, "reference_designator must not be empty"));
        }
        component.reference_designator = new.to_string();
    } else if let Some(property) = property_name(&change.field) {
        let new = new_string(change)?;
        set_property(component, property, &Value::String(new.to_string()), change)?;
    } else {
        return Err(EditError::UnknownField {
            target: change.target.clone(),
            field: change.field.clone(),
        });
    }

    Ok(AppliedChange {
        target: change.target.clone(),
        field: change.field.clone(),
        old_value: change.old_value.clone(),
        new_value: change.new_value.clone(),
        object_id: Some(object_id),
    })
}

fn restore_change(
    project: &mut Project,
    change: &AppliedChange,
    undo: bool,
) -> Result<(), EditError> {
    let expected = if undo { &change.new_value } else { &change.old_value };
    let replacement = if undo { &change.old_value } else { &change.new_value };

    if change.target == "project" {
        let current = project_value(project, &change.field)?;
        if current != *expected {
            return Err(stale(change));
        }
        let text = replacement
            .as_str()
            .ok_or_else(|| invalid_applied(change, "value must be a string"))?;
        set_project_value(project, &change.field, text);
        return Ok(());
    }

    let index = resolve_component(project, change)?;
    let component = &mut project.schematic.components[index];

    if change.field == "reference_designator" {
        let current = Value::String(component.reference_designator.clone());
        if current != *expected {
            return Err(stale(change));
        }
        let text = replacement
            .as_str()
            .ok_or_else(|| invalid_applied(change, "value must be a string"))?;
        if text.is_empty() {
            return Err(invalid_applied(change, "reference_designator must not be empty"));
        }
        component.reference_designator = text.to_string();
        return Ok(());
    }

    let Some(property) = property_name(&change.field) else {
        return Err(EditError::UnknownField {
            target: change.target.clone(),
            field: change.field.clone(),
        });
    };
    let current = property_value(component, property);
    if current != *expected {
        return Err(stale(change));
    }
    set_property_applied(component, property, replacement, change)
}

fn resolve_component(project: &Project, change: &AppliedChange) -> Result<usize, EditError> {
    if let Some(id) = change.object_id {
        return project
            .schematic
            .components
            .iter()
            .position(|component| component.id == id)
            .ok_or_else(|| EditError::UnknownTarget(change.target.clone()));
    }

    let reference = component_reference(&change.target)?;
    project
        .schematic
        .components
        .iter()
        .position(|component| component.reference_designator == reference)
        .ok_or_else(|| EditError::UnknownTarget(change.target.clone()))
}

fn component_reference(target: &str) -> Result<&str, EditError> {
    target
        .strip_prefix("component:")
        .filter(|reference| !reference.is_empty())
        .ok_or_else(|| EditError::UnknownTarget(target.to_string()))
}

fn property_name(field: &str) -> Option<&str> {
    match field {
        "value" => Some("Value"),
        other => other.strip_prefix("property:").filter(|name| !name.is_empty()),
    }
}

fn project_value(project: &Project, field: &str) -> Result<Value, EditError> {
    let value = match field {
        "name" => &project.name,
        "version" => &project.version,
        "author" => &project.metadata.author,
        "description" => &project.metadata.description,
        _ => {
            return Err(EditError::UnknownField {
                target: "project".to_string(),
                field: field.to_string(),
            });
        }
    };
    Ok(Value::String(value.clone()))
}

fn set_project_value(project: &mut Project, field: &str, value: &str) {
    match field {
        "name" => project.name = value.to_string(),
        "version" => project.version = value.to_string(),
        "author" => project.metadata.author = value.to_string(),
        "description" => project.metadata.description = value.to_string(),
        _ => {}
    }
}

fn component_value(component: &Component, target: &str, field: &str) -> Result<Value, EditError> {
    if field == "reference_designator" {
        return Ok(Value::String(component.reference_designator.clone()));
    }
    match property_name(field) {
        Some(property) => Ok(property_value(component, property)),
        None => {
            Err(EditError::UnknownField { target: target.to_string(), field: field.to_string() })
        }
    }
}

fn property_value(component: &Component, name: &str) -> Value {
    component
        .properties
        .iter()
        .find(|property| property.name == name)
        .map(|property| Value::String(property.value.clone()))
        .unwrap_or(Value::Null)
}

fn set_property(
    component: &mut Component,
    name: &str,
    value: &Value,
    change: &ChangeProposal,
) -> Result<(), EditError> {
    let text = value.as_str().ok_or_else(|| invalid(change, "value must be a string"))?;
    if let Some(property) = component.properties.iter_mut().find(|property| property.name == name) {
        property.value = text.to_string();
    } else {
        component
            .properties
            .push(ComponentProperty { name: name.to_string(), value: text.to_string() });
    }
    Ok(())
}

fn set_property_applied(
    component: &mut Component,
    name: &str,
    value: &Value,
    change: &AppliedChange,
) -> Result<(), EditError> {
    if value.is_null() {
        component.properties.retain(|property| property.name != name);
        return Ok(());
    }
    let text = value.as_str().ok_or_else(|| invalid_applied(change, "value must be a string"))?;
    if let Some(property) = component.properties.iter_mut().find(|property| property.name == name) {
        property.value = text.to_string();
    } else {
        component
            .properties
            .push(ComponentProperty { name: name.to_string(), value: text.to_string() });
    }
    Ok(())
}

fn ensure_old(change: &ChangeProposal, current: &Value) -> Result<(), EditError> {
    if *current != change.old_value {
        return Err(EditError::StaleChange {
            target: change.target.clone(),
            field: change.field.clone(),
        });
    }
    Ok(())
}

fn new_string(change: &ChangeProposal) -> Result<&str, EditError> {
    change.new_value.as_str().ok_or_else(|| invalid(change, "new_value must be a string"))
}

fn invalid(change: &ChangeProposal, message: &str) -> EditError {
    EditError::InvalidValue {
        target: change.target.clone(),
        field: change.field.clone(),
        message: message.to_string(),
    }
}

fn invalid_applied(change: &AppliedChange, message: &str) -> EditError {
    EditError::InvalidValue {
        target: change.target.clone(),
        field: change.field.clone(),
        message: message.to_string(),
    }
}

fn stale(change: &AppliedChange) -> EditError {
    EditError::StaleChange { target: change.target.clone(), field: change.field.clone() }
}

#[cfg(test)]
mod tests {
    use super::{EditError, apply, redo, undo};
    use crate::{Component, ComponentProperty, Project, proposal::ChangeProposal};
    use serde_json::json;
    use uuid::Uuid;

    fn fixture() -> Project {
        let mut project = Project::new("demo");
        project.schematic.components.push(Component {
            id: Uuid::from_u128(1),
            reference_designator: "R1".to_string(),
            properties: vec![
                ComponentProperty { name: "Value".to_string(), value: "10k".to_string() },
                ComponentProperty { name: "Tolerance".to_string(), value: "5%".to_string() },
            ],
            ..Component::default()
        });
        project
    }

    fn change(target: &str, field: &str, old: &str, new: &str) -> ChangeProposal {
        ChangeProposal::new(target, field, json!(old), json!(new))
    }

    fn component(project: &Project) -> &Component {
        &project.schematic.components[0]
    }

    fn property<'a>(project: &'a Project, name: &str) -> Option<&'a str> {
        component(project)
            .properties
            .iter()
            .find(|property| property.name == name)
            .map(|property| property.value.as_str())
    }

    #[test]
    fn apply_updates_a_component_value() {
        let mut project = fixture();
        let transaction =
            apply(&mut project, "widen R1", &[change("component:R1", "value", "10k", "22k")])
                .expect("edit applies");

        assert_eq!(property(&project, "Value"), Some("22k"));
        assert_eq!(transaction.label, "widen R1");
        assert_eq!(transaction.changes.len(), 1);
        assert!(!transaction.id.is_nil());
    }

    #[test]
    fn creating_a_missing_value_is_undone_by_removing_it() {
        let mut project = fixture();
        project.schematic.components[0].properties.clear();
        let before = serde_json::to_string(&project).expect("serialize");
        let transaction = apply(
            &mut project,
            "add value",
            &[ChangeProposal::new("component:R1", "value", serde_json::Value::Null, json!("22k"))],
        )
        .expect("value is created");

        assert_eq!(property(&project, "Value"), Some("22k"));
        undo(&mut project, &transaction).expect("undo");
        assert_eq!(property(&project, "Value"), None);
        assert_eq!(before, serde_json::to_string(&project).expect("serialize"));
    }

    #[test]
    fn apply_updates_an_arbitrary_property() {
        let mut project = fixture();
        apply(
            &mut project,
            "tolerance",
            &[change("component:R1", "property:Tolerance", "5%", "1%")],
        )
        .expect("edit applies");

        assert_eq!(property(&project, "Tolerance"), Some("1%"));
    }

    #[test]
    fn apply_updates_project_level_fields() {
        let mut project = fixture();
        apply(
            &mut project,
            "rename",
            &[
                change("project", "name", "demo", "production"),
                change("project", "version", "1.0", "1.1"),
            ],
        )
        .expect("project edits apply");

        assert_eq!(project.name, "production");
        assert_eq!(project.version, "1.1");
    }

    #[test]
    fn unknown_target_is_rejected_without_mutation() {
        let mut project = fixture();
        let before = serde_json::to_string(&project).expect("serialize");

        let error = apply(&mut project, "bad", &[change("component:R9", "value", "10k", "22k")])
            .expect_err("unknown target");

        assert!(matches!(error, EditError::UnknownTarget(target) if target == "component:R9"));
        assert_eq!(before, serde_json::to_string(&project).expect("serialize"));
    }

    #[test]
    fn unknown_field_is_rejected() {
        let mut project = fixture();
        let error = apply(&mut project, "bad", &[change("component:R1", "bogus", "a", "b")])
            .expect_err("unknown field");

        assert!(matches!(error, EditError::UnknownField { field, .. } if field == "bogus"));
    }

    #[test]
    fn stale_old_value_is_rejected() {
        let mut project = fixture();
        let before = serde_json::to_string(&project).expect("serialize");
        let error = apply(&mut project, "stale", &[change("component:R1", "value", "1k", "22k")])
            .expect_err("stale change");

        assert!(matches!(error, EditError::StaleChange { .. }));
        assert_eq!(before, serde_json::to_string(&project).expect("serialize"));
    }

    #[test]
    fn wrong_json_type_is_rejected() {
        let mut project = fixture();
        let error = apply(
            &mut project,
            "wrong type",
            &[ChangeProposal::new("component:R1", "value", json!("10k"), json!(22))],
        )
        .expect_err("number is not a string");

        assert!(matches!(error, EditError::InvalidValue { .. }));
    }

    #[test]
    fn a_multi_change_failure_is_atomic() {
        let mut project = fixture();
        let before = serde_json::to_string(&project).expect("serialize");
        let error = apply(
            &mut project,
            "atomic",
            &[
                change("project", "name", "demo", "changed"),
                change("component:R1", "value", "1k", "22k"),
            ],
        )
        .expect_err("second change is stale");

        assert!(matches!(error, EditError::StaleChange { .. }));
        assert_eq!(before, serde_json::to_string(&project).expect("serialize"));
        assert_eq!(project.name, "demo");
    }

    #[test]
    fn undo_restores_the_exact_starting_json() {
        let mut project = fixture();
        let before = serde_json::to_string(&project).expect("serialize");
        let transaction = apply(
            &mut project,
            "two edits",
            &[
                change("component:R1", "value", "10k", "22k"),
                change("project", "name", "demo", "renamed"),
            ],
        )
        .expect("edit applies");

        undo(&mut project, &transaction).expect("undo");
        assert_eq!(before, serde_json::to_string(&project).expect("serialize"));
    }

    #[test]
    fn redo_after_undo_restores_the_edited_json() {
        let mut project = fixture();
        let transaction =
            apply(&mut project, "edit", &[change("component:R1", "value", "10k", "22k")])
                .expect("edit applies");
        let edited = serde_json::to_string(&project).expect("serialize");

        undo(&mut project, &transaction).expect("undo");
        redo(&mut project, &transaction).expect("redo");

        assert_eq!(edited, serde_json::to_string(&project).expect("serialize"));
        assert_eq!(property(&project, "Value"), Some("22k"));
    }

    #[test]
    fn undo_and_redo_reject_stale_state() {
        let mut project = fixture();
        let transaction =
            apply(&mut project, "edit", &[change("component:R1", "value", "10k", "22k")])
                .expect("edit applies");

        project.schematic.components[0].properties[0].value = "33k".to_string();
        let error = undo(&mut project, &transaction).expect_err("undo is stale");
        assert!(matches!(error, EditError::StaleChange { .. }));
        assert_eq!(property(&project, "Value"), Some("33k"));

        project.schematic.components[0].properties[0].value = "22k".to_string();
        undo(&mut project, &transaction).expect("undo");
        project.schematic.components[0].properties[0].value = "33k".to_string();
        let error = redo(&mut project, &transaction).expect_err("redo is stale");
        assert!(matches!(error, EditError::StaleChange { .. }));
        assert_eq!(property(&project, "Value"), Some("33k"));
    }

    #[test]
    fn transaction_ids_are_repeatable_uuid_v5_values() {
        let mut first = fixture();
        let mut second = fixture();
        let changes = [change("component:R1", "value", "10k", "22k")];

        let left = apply(&mut first, "same edit", &changes).expect("first edit");
        let right = apply(&mut second, "same edit", &changes).expect("second edit");

        assert_eq!(left.id, right.id);
        assert_eq!(left.id.get_version_num(), 5);
        assert_ne!(left.id, Uuid::nil());
    }

    #[test]
    fn reference_designator_changes_undo_by_component_id() {
        let mut project = fixture();
        let transaction = apply(
            &mut project,
            "rename R1",
            &[
                change("component:R1", "reference_designator", "R1", "R2"),
                change("component:R1", "value", "10k", "22k"),
            ],
        )
        .expect("rename applies");

        assert_eq!(component(&project).reference_designator, "R2");
        undo(&mut project, &transaction).expect("undo");
        assert_eq!(component(&project).reference_designator, "R1");
        assert_eq!(property(&project, "Value"), Some("10k"));

        redo(&mut project, &transaction).expect("redo");
        assert_eq!(component(&project).reference_designator, "R2");
        assert_eq!(property(&project, "Value"), Some("22k"));
    }
}
