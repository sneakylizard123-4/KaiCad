#[cfg(test)]
mod tests {
    use crate::{Project, ProjectMetadata};
    use crate::component::Component;
    use crate::symbol::Symbol;
    use crate::net::{Net, NetClass};
    use crate::sheet::Sheet;
    use crate::Uuid;
    use serde_json;
    
    #[test]
    fn test_project_default() {
        let project = Project::default();
        assert_eq!(project.name, "");
        assert_eq!(project.version, "1.0");
        assert!(project.id != Uuid::nil());
    }
    
    #[test]
    fn test_project_serialization() {
        let mut project = Project::default();
        project.name = "Test Project".to_string();
        project.version = "0.1.0".to_string();
        
        let serialized = serde_json::to_string(&project).unwrap();
        let deserialized: Project = serde_json::from_str(&serialized).unwrap();
        
        assert_eq!(deserialized.name, "Test Project");
        assert_eq!(deserialized.version, "0.1.0");
        assert_eq!(project.id, deserialized.id);
    }
    
    #[test]
    fn test_symbol_creation() {
        let mut symbol = Symbol::default();
        symbol.id = Uuid::new_v4();
        symbol.name = "TEST_RESISTOR".to_string();
        symbol.library = "Device".to_string();
        
        assert_eq!(symbol.name, "TEST_RESISTOR");
        assert_eq!(symbol.library, "Device");
        assert!(symbol.id != Uuid::nil());
    }
    
    #[test]
    fn test_net_creation() {
        let mut net = Net::default();
        net.id = Uuid::new_v4();
        net.name = "VCC".to_string();
        net.netclass = NetClass::Power;
        
        assert_eq!(net.name, "VCC");
        assert_eq!(net.netclass, NetClass::Power);
        assert!(net.id != Uuid::nil());
    }
    
    #[test]
    fn test_sheet_creation() {
        let mut sheet = Sheet::default();
        sheet.id = Uuid::new_v4();
        sheet.name = "Sheet1".to_string();
        
        assert_eq!(sheet.name, "Sheet1");
        assert!(sheet.id != Uuid::nil());
        assert!(sheet.parent.is_none());
        assert!(sheet.children.is_empty());
    }
    
    #[test]
    fn test_component_creation() {
        let mut component = Component::default();
        component.id = Uuid::new_v4();
        component.reference_designator = "R1".to_string();
        component.symbol_id = Uuid::new_v4();
        component.footprint_id = Uuid::new_v4();
        
        assert_eq!(component.reference_designator, "R1");
        assert!(component.id != Uuid::nil());
        assert!(component.symbol_id != Uuid::nil());
        assert!(component.footprint_id != Uuid::nil());
    }
}