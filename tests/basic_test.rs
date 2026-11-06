#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_project_creation() {
        let mut project = kaicas_core::Project::default();
        project.name = "Test Project".to_string();
        project.version = "0.1.0".to_string();
        assert_eq!(project.name, "Test Project");
        assert_eq!(project.version, "0.1.0");
    }
    
    #[test]
    fn test_symbol_creation() {
        let mut symbol = kaicas_core::Symbol::default();
        symbol.id = kaicas_core::uuid::Uuid::new_v4();
        symbol.name = "TEST_SYMBOL".to_string();
        symbol.library = "TestLib".to_string();
        assert_eq!(symbol.name, "TEST_SYMBOL");
        assert_eq!(symbol.library, "TestLib");
    }
    
    #[test]
    fn test_footprint_creation() {
        let mut footprint = kaicas_pcb::footprint::Footprint::default();
        footprint.name = "TEST_FOOTPRINT".to_string();
        footprint.reference_designator = "R1".to_string();
        assert_eq!(footprint.name, "TEST_FOOTPRINT");
        assert_eq!(footprint.reference_designator, "R1");
    }
    
    #[test]
    fn test_serialization() {
        let mut project = kaicas_core::Project::default();
        project.name = "Serialization Test".to_string();
        project.version = "1.0.0".to_string();
        
        let serialized = serde_json::to_string(&project).unwrap();
        let deserialized: kaicas_core::Project = serde_json::from_str(&serialized).unwrap();
        
        assert_eq!(deserialized.name, "Serialization Test");
        assert_eq!(deserialized.version, "1.0.0");
    }
}