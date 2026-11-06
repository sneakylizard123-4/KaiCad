fn main() {
    // Test core project creation
    let mut project = kaicas_core::Project::default();
    project.name = "Test Project".to_string();
    project.version = "0.1.0".to_string();
    
    // Test schematic symbol creation
    let mut symbol = kaicas_core::Symbol::default();
    symbol.id = kaicas_core::uuid::Uuid::new_v4();
    symbol.name = "TEST_SYMBOL".to_string();
    symbol.library = "TestLib".to_string();
    
    // Test PCB footprint creation
    let mut footprint = kaicas_pcb::footprint::Footprint::default();
    footprint.name = "TEST_FOOTPRINT".to_string();
    footprint.reference_designator = "R1".to_string();
    
    // Test serialization
    let json = serde_json::to_string_pretty(&project).unwrap();
    println!("Project JSON: {}", json);
    
    // Test deserialization
    let decoded: kaicas_core::Project = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded.name, "Test Project");
    
    println!("All basic tests passed!");
}