//! CRUD example for the KaiCad engineering database.

use uuid::Uuid;

fn main() {
    let path = std::env::temp_dir().join(format!("kai-cad-store-crud-{}.db", Uuid::new_v4()));
    println!("open: {}", path.display());

    let mut store = store::Store::open(&path).expect("open database");
    println!("schema version: {}", store.schema_version().expect("schema version"));

    let mut project = core::Project::new("store-demo");
    project.schematic.nets.push(core::Net {
        id: Uuid::new_v4(),
        name: "NET_DEMO".to_string(),
        pins: Vec::new(),
        netclass: core::NetClass::Signal,
    });
    project.schematic.symbols.push(core::Symbol {
        id: Uuid::new_v4(),
        name: "DEMO".to_string(),
        pins: Vec::new(),
        library: "demo-lib".to_string(),
    });

    store.create_project(&project).expect("create project");
    println!("created: {}", project.id);

    let summaries = store.list_projects().expect("list projects");
    println!("listed: {} project(s)", summaries.len());

    let loaded = store.load_project(project.id).expect("load project").expect("project exists");
    println!(
        "loaded: {} ({} nets, {} symbols)",
        loaded.name,
        loaded.schematic.nets.len(),
        loaded.schematic.symbols.len()
    );

    let mut updated = loaded.clone();
    updated.name = "store-demo-renamed".to_string();
    updated.metadata.author = "crud example".to_string();
    let was_updated = store.update_project(&updated).expect("update project");
    println!("updated: {}", was_updated);

    let reloaded = store.load_project(project.id).expect("reload project").expect("project exists");
    println!("reloaded: {} by {}", reloaded.name, reloaded.metadata.author);

    let was_deleted = store.delete_project(project.id).expect("delete project");
    println!("deleted: {}", was_deleted);

    assert!(store.load_project(project.id).expect("final load").is_none());

    std::fs::remove_file(&path).expect("remove database file");
    println!("store example: ok");
}
