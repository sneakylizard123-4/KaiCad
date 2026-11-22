// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 KaiCad contributors

//! Local plugin metadata for KaiCad.
//!
//! This crate validates plugin manifests, keeps a deterministic registry and
//! discovers manifests in a local directory. It never loads, executes or copies
//! plugin code, and it never checks whether an entry point exists.

use std::collections::HashSet;
use std::fs;
use std::path::{Component, Path};

/// The only plugin API version this build understands.
pub const SUPPORTED_API_VERSION: u32 = 1;

/// A validated description of an installed plugin.
///
/// Manifests are plain data. Constructing one does not load code; call
/// [`PluginManifest::validate`] or [`PluginRegistry::register`] before treating
/// it as trusted metadata.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PluginManifest {
    /// Stable plugin identity, limited to lowercase ASCII letters, digits,
    /// `.`, `_` and `-`. It must start with a letter or digit.
    pub id: String,
    /// Human-readable plugin name.
    pub name: String,
    /// Plugin version string. Semantic-version parsing is not performed.
    pub version: String,
    /// Plugin API version; must equal [`SUPPORTED_API_VERSION`].
    pub api_version: u32,
    /// Relative path to the plugin entry point. It is validated but never
    /// opened, resolved or executed.
    pub entry_point: String,
    /// Subsystems this plugin declares. At least one unique value is required.
    pub capabilities: Vec<PluginCapability>,
}

/// A subsystem a plugin declares support for.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PluginCapability {
    Schematic,
    Pcb,
    Ai,
    ImportExport,
}

/// Errors produced while validating manifests or discovering plugins.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginError {
    /// A manifest field failed validation.
    InvalidManifest { field: String, message: String },
    /// The manifest targets an API version this build does not support.
    UnsupportedApiVersion { found: u32, supported: u32 },
    /// Another manifest in the registry or discovery set has this id.
    DuplicateId(String),
    /// The entry point is absolute or escapes its plugin directory.
    UnsafeEntryPoint(String),
    /// Manifest JSON could not be encoded or decoded.
    Json(String),
    /// A manifest or directory could not be read or written.
    Io(String),
}

impl std::fmt::Display for PluginError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidManifest { field, message } => {
                write!(formatter, "invalid plugin manifest field `{field}`: {message}")
            }
            Self::UnsupportedApiVersion { found, supported } => write!(
                formatter,
                "unsupported plugin API version {found}; supported version is {supported}"
            ),
            Self::DuplicateId(id) => write!(formatter, "plugin id `{id}` is already registered"),
            Self::UnsafeEntryPoint(path) => write!(formatter, "unsafe plugin entry point `{path}`"),
            Self::Json(message) => write!(formatter, "invalid plugin manifest JSON: {message}"),
            Self::Io(message) => write!(formatter, "plugin manifest I/O error: {message}"),
        }
    }
}

impl std::error::Error for PluginError {}

fn invalid(field: &str, message: impl Into<String>) -> PluginError {
    PluginError::InvalidManifest { field: field.to_string(), message: message.into() }
}

impl PluginManifest {
    /// Validate every manifest field without touching the filesystem.
    pub fn validate(&self) -> Result<(), PluginError> {
        let mut id_chars = self.id.chars();
        let valid_start = id_chars
            .next()
            .is_some_and(|first| first.is_ascii_lowercase() || first.is_ascii_digit());
        let valid_rest = id_chars.all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || matches!(character, '.' | '_' | '-')
        });
        if !valid_start || !valid_rest {
            return Err(invalid(
                "id",
                "must start with a lowercase ASCII letter or digit and contain only lowercase ASCII \
                 letters, digits, `.`, `_` or `-`",
            ));
        }

        if self.name.trim().is_empty() {
            return Err(invalid("name", "must not be empty"));
        }
        if self.version.trim().is_empty() {
            return Err(invalid("version", "must not be empty"));
        }
        if self.api_version != SUPPORTED_API_VERSION {
            return Err(PluginError::UnsupportedApiVersion {
                found: self.api_version,
                supported: SUPPORTED_API_VERSION,
            });
        }

        validate_entry_point(&self.entry_point)?;

        if self.capabilities.is_empty() {
            return Err(invalid("capabilities", "must contain at least one capability"));
        }
        let mut seen = HashSet::new();
        for capability in &self.capabilities {
            if !seen.insert(*capability) {
                return Err(invalid(
                    "capabilities",
                    format!("duplicate capability `{capability:?}`"),
                ));
            }
        }

        Ok(())
    }

    /// Parse and validate a manifest from compact JSON.
    pub fn from_json(text: &str) -> Result<Self, PluginError> {
        let manifest: Self =
            serde_json::from_str(text).map_err(|error| PluginError::Json(error.to_string()))?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// Validate and encode a manifest as compact JSON.
    pub fn to_json(&self) -> Result<String, PluginError> {
        self.validate()?;
        serde_json::to_string(self).map_err(|error| PluginError::Json(error.to_string()))
    }

    /// Validate and save a manifest as pretty JSON with one trailing newline.
    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), PluginError> {
        self.validate()?;
        let json = serde_json::to_string_pretty(self)
            .map_err(|error| PluginError::Json(error.to_string()))?;
        let path = path.as_ref();
        fs::write(path, format!("{json}\n"))
            .map_err(|error| PluginError::Io(format!("{}: {error}", path.display())))
    }

    /// Load and validate a manifest from a file.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, PluginError> {
        let path = path.as_ref();
        let text = fs::read_to_string(path)
            .map_err(|error| PluginError::Io(format!("{}: {error}", path.display())))?;
        Self::from_json(&text)
    }
}

fn validate_entry_point(entry_point: &str) -> Result<(), PluginError> {
    let unsafe_path = || PluginError::UnsafeEntryPoint(entry_point.to_string());

    if entry_point.trim().is_empty()
        || entry_point.contains('\\')
        || entry_point.contains(':')
        || entry_point.chars().any(char::is_control)
        || Path::new(entry_point).is_absolute()
    {
        return Err(unsafe_path());
    }
    if entry_point.split('/').any(str::is_empty) {
        return Err(unsafe_path());
    }
    if !Path::new(entry_point)
        .components()
        .all(|component| matches!(component, Component::Normal(_)))
    {
        return Err(unsafe_path());
    }

    Ok(())
}

/// A duplicate-free registry of validated manifests, sorted by plugin id.
#[derive(Debug, Clone, Default)]
pub struct PluginRegistry {
    plugins: Vec<PluginManifest>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Validate and insert a manifest. Duplicate ids and invalid manifests
    /// leave the registry unchanged.
    pub fn register(&mut self, manifest: PluginManifest) -> Result<(), PluginError> {
        manifest.validate()?;
        match self.plugins.binary_search_by(|entry| entry.id.cmp(&manifest.id)) {
            Ok(_) => Err(PluginError::DuplicateId(manifest.id)),
            Err(index) => {
                self.plugins.insert(index, manifest);
                Ok(())
            }
        }
    }

    /// Remove a plugin by id, returning whether it was present.
    pub fn remove(&mut self, id: &str) -> bool {
        match self.plugins.binary_search_by(|entry| entry.id.as_str().cmp(id)) {
            Ok(index) => {
                self.plugins.remove(index);
                true
            }
            Err(_) => false,
        }
    }

    /// Find a plugin by id.
    pub fn find(&self, id: &str) -> Option<&PluginManifest> {
        let index = self.plugins.binary_search_by(|entry| entry.id.as_str().cmp(id)).ok()?;
        self.plugins.get(index)
    }

    /// Return every manifest in ascending id order.
    pub fn list(&self) -> Vec<&PluginManifest> {
        self.plugins.iter().collect()
    }

    pub fn len(&self) -> usize {
        self.plugins.len()
    }

    pub fn is_empty(&self) -> bool {
        self.plugins.is_empty()
    }
}

/// Discover and validate every `*.json` plugin manifest directly inside
/// `directory`.
///
/// Entries are read in file-name order, so a malformed file produces the same
/// error on every run. Results are returned in ascending `(id, entry_point)`
/// order. Non-JSON files and subdirectories are ignored. A missing directory,
/// or a directory with no JSON manifests, returns [`PluginError::Io`].
pub fn discover_manifests(directory: impl AsRef<Path>) -> Result<Vec<PluginManifest>, PluginError> {
    let directory = directory.as_ref();
    let entries = fs::read_dir(directory)
        .map_err(|error| PluginError::Io(format!("{}: {error}", directory.display())))?;

    let mut paths = Vec::new();
    for entry in entries {
        let entry =
            entry.map_err(|error| PluginError::Io(format!("{}: {error}", directory.display())))?;
        let file_type = entry
            .file_type()
            .map_err(|error| PluginError::Io(format!("{}: {error}", entry.path().display())))?;
        if !file_type.is_file() {
            continue;
        }
        let path = entry.path();
        if path.extension().is_some_and(|extension| extension == "json") {
            paths.push(path);
        }
    }

    if paths.is_empty() {
        return Err(PluginError::Io(format!(
            "no plugin manifests found in {}",
            directory.display()
        )));
    }
    paths.sort_by(|left, right| left.file_name().cmp(&right.file_name()));

    let mut manifests = Vec::with_capacity(paths.len());
    let mut seen_ids = HashSet::new();
    for path in paths {
        let manifest = PluginManifest::load(&path)?;
        if !seen_ids.insert(manifest.id.clone()) {
            return Err(PluginError::DuplicateId(manifest.id));
        }
        manifests.push(manifest);
    }
    manifests.sort_by(|left, right| {
        left.id.cmp(&right.id).then_with(|| left.entry_point.cmp(&right.entry_point))
    });

    Ok(manifests)
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(label: &str) -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let nanos =
                SystemTime::now().duration_since(UNIX_EPOCH).expect("clock after epoch").as_nanos();
            let path = std::env::temp_dir().join(format!(
                "kaicad-plugin-{label}-{}-{nanos}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).expect("create temp dir");
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn manifest(id: &str) -> PluginManifest {
        PluginManifest {
            id: id.to_string(),
            name: "Example plugin".to_string(),
            version: "0.1.0".to_string(),
            api_version: SUPPORTED_API_VERSION,
            entry_point: "plugins/example.so".to_string(),
            capabilities: vec![PluginCapability::Schematic, PluginCapability::Ai],
        }
    }

    fn write_manifest(directory: &Path, file_name: &str, manifest: &PluginManifest) {
        manifest.save(directory.join(file_name)).expect("save manifest");
    }

    #[test]
    fn manifest_round_trips_through_json() {
        let original = manifest("com.example.alpha");

        let json = original.to_json().expect("encode manifest");
        assert!(!json.ends_with('\n'));
        let restored = PluginManifest::from_json(&json).expect("decode manifest");

        assert_eq!(restored, original);
        assert_eq!(restored.capabilities, vec![PluginCapability::Schematic, PluginCapability::Ai]);
    }

    #[test]
    fn manifest_save_and_load_round_trip() {
        let directory = TempDir::new("save-load");
        let path = directory.path().join("manifest.json");
        let original = manifest("com.example.saved");

        original.save(&path).expect("save manifest");
        let text = fs::read_to_string(&path).expect("read manifest");
        assert!(text.ends_with('\n'));
        let restored = PluginManifest::load(&path).expect("load manifest");

        assert_eq!(restored, original);
    }

    #[test]
    fn manifest_rejects_invalid_ids() {
        for id in ["", "Upper", ".hidden", "-dash", "has space", "slash/id", "ümlaut"] {
            let mut candidate = manifest("valid");
            candidate.id = id.to_string();
            let error = candidate.validate().expect_err("invalid id");
            assert!(matches!(
                error,
                PluginError::InvalidManifest { ref field, .. } if field == "id"
            ));
        }
    }

    #[test]
    fn manifest_rejects_empty_name_and_version() {
        let mut no_name = manifest("com.example.no-name");
        no_name.name = "   ".to_string();
        assert!(matches!(
            no_name.validate(),
            Err(PluginError::InvalidManifest { ref field, .. }) if field == "name"
        ));

        let mut no_version = manifest("com.example.no-version");
        no_version.version = String::new();
        assert!(matches!(
            no_version.validate(),
            Err(PluginError::InvalidManifest { ref field, .. }) if field == "version"
        ));
    }

    #[test]
    fn manifest_rejects_unsupported_api_version() {
        let mut candidate = manifest("com.example.future");
        candidate.api_version = SUPPORTED_API_VERSION + 1;

        assert_eq!(
            candidate.validate(),
            Err(PluginError::UnsupportedApiVersion {
                found: SUPPORTED_API_VERSION + 1,
                supported: SUPPORTED_API_VERSION,
            })
        );
    }

    #[test]
    fn manifest_accepts_relative_entry_points() {
        for entry_point in ["plugin.so", "bin/plugin.so", "a.b/c-d_e/plugin"] {
            let mut candidate = manifest("com.example.entry");
            candidate.entry_point = entry_point.to_string();
            candidate.validate().expect("relative entry point");
        }
    }

    #[test]
    fn manifest_rejects_unsafe_entry_points() {
        for entry_point in [
            "",
            "../plugin.so",
            "bin/../plugin.so",
            "/usr/lib/plugin.so",
            "C:/plugin.so",
            "C:plugin.so",
            "bin\\plugin.so",
            "bin//plugin.so",
            "./plugin.so",
            "bin/",
            ".",
        ] {
            let mut candidate = manifest("com.example.entry");
            candidate.entry_point = entry_point.to_string();
            assert!(
                matches!(candidate.validate(), Err(PluginError::UnsafeEntryPoint(_))),
                "entry point `{entry_point}` was accepted"
            );
        }
    }

    #[test]
    fn manifest_rejects_empty_and_duplicate_capabilities() {
        let mut empty = manifest("com.example.empty");
        empty.capabilities.clear();
        assert!(matches!(
            empty.validate(),
            Err(PluginError::InvalidManifest { ref field, .. }) if field == "capabilities"
        ));

        let mut duplicate = manifest("com.example.duplicate");
        duplicate.capabilities = vec![PluginCapability::Pcb, PluginCapability::Pcb];
        let error = duplicate.validate().expect_err("duplicate capabilities");
        assert!(matches!(
            error,
            PluginError::InvalidManifest { ref field, .. } if field == "capabilities"
        ));
    }

    #[test]
    fn manifest_json_and_io_errors_are_reported() {
        assert!(matches!(PluginManifest::from_json("{"), Err(PluginError::Json(_))));

        let missing = std::env::temp_dir().join("kaicad-plugin-does-not-exist.json");
        assert!(matches!(PluginManifest::load(missing), Err(PluginError::Io(_))));
    }

    #[test]
    fn registry_sorts_by_id() {
        let mut registry = PluginRegistry::new();
        for id in ["com.example.zeta", "com.example.alpha", "com.example.middle"] {
            registry.register(manifest(id)).expect("register manifest");
        }

        let ids: Vec<_> = registry.list().iter().map(|entry| entry.id.as_str()).collect();
        assert_eq!(ids, vec!["com.example.alpha", "com.example.middle", "com.example.zeta"]);
        assert_eq!(registry.len(), 3);
        assert!(!registry.is_empty());
        assert_eq!(
            registry.find("com.example.middle").map(|entry| entry.id.as_str()),
            Some("com.example.middle")
        );
    }

    #[test]
    fn duplicate_registration_is_failure_atomic() {
        let mut registry = PluginRegistry::new();
        registry.register(manifest("com.example.one")).expect("register original");

        let mut replacement = manifest("com.example.one");
        replacement.name = "Replacement".to_string();
        assert_eq!(
            registry.register(replacement),
            Err(PluginError::DuplicateId("com.example.one".to_string()))
        );

        assert_eq!(registry.len(), 1);
        assert_eq!(
            registry.find("com.example.one").expect("original survives").name,
            "Example plugin"
        );
    }

    #[test]
    fn invalid_registration_is_failure_atomic() {
        let mut registry = PluginRegistry::new();
        let mut invalid_manifest = manifest("com.example.invalid");
        invalid_manifest.entry_point = "../escape.so".to_string();

        assert!(matches!(
            registry.register(invalid_manifest),
            Err(PluginError::UnsafeEntryPoint(_))
        ));
        assert!(registry.is_empty());
    }

    #[test]
    fn registry_remove_find_and_empty() {
        let mut registry = PluginRegistry::new();
        registry.register(manifest("com.example.remove")).expect("register manifest");

        assert!(registry.remove("com.example.remove"));
        assert!(!registry.remove("com.example.remove"));
        assert!(registry.find("com.example.remove").is_none());
        assert!(registry.is_empty());
    }

    #[test]
    fn discovery_sorts_manifest_files_and_returns_sorted_ids() {
        let directory = TempDir::new("discovery-sorted");
        write_manifest(directory.path(), "zeta.json", &manifest("com.example.zeta"));
        write_manifest(directory.path(), "alpha.json", &manifest("com.example.alpha"));

        let discovered = discover_manifests(directory.path()).expect("discover manifests");
        let ids: Vec<_> = discovered.iter().map(|entry| entry.id.as_str()).collect();

        assert_eq!(ids, vec!["com.example.alpha", "com.example.zeta"]);
        assert_eq!(discovered[0].capabilities.len(), 2);
    }

    #[test]
    fn discovery_ignores_non_json_files() {
        let directory = TempDir::new("discovery-non-json");
        write_manifest(directory.path(), "manifest.json", &manifest("com.example.only"));
        fs::write(directory.path().join("notes.txt"), "not a manifest").expect("write notes");
        fs::create_dir_all(directory.path().join("nested.json")).expect("create directory");

        let discovered = discover_manifests(directory.path()).expect("discover manifests");
        assert_eq!(discovered.len(), 1);
        assert_eq!(discovered[0].id, "com.example.only");
    }

    #[test]
    fn discovery_rejects_duplicate_ids() {
        let directory = TempDir::new("discovery-duplicate");
        write_manifest(directory.path(), "a.json", &manifest("com.example.same"));
        write_manifest(directory.path(), "b.json", &manifest("com.example.same"));

        assert_eq!(
            discover_manifests(directory.path()),
            Err(PluginError::DuplicateId("com.example.same".to_string()))
        );
    }

    #[test]
    fn discovery_rejects_invalid_files_instead_of_skipping_them() {
        let directory = TempDir::new("discovery-invalid");
        fs::write(directory.path().join("a-broken.json"), "{ not json").expect("write invalid");
        write_manifest(directory.path(), "b-valid.json", &manifest("com.example.valid"));

        assert!(matches!(discover_manifests(directory.path()), Err(PluginError::Json(_))));
    }

    #[test]
    fn discovery_rejects_missing_or_empty_directories() {
        let directory = TempDir::new("discovery-empty");
        assert!(matches!(
            discover_manifests(directory.path().join("missing")),
            Err(PluginError::Io(_))
        ));
        assert!(matches!(discover_manifests(directory.path()), Err(PluginError::Io(_))));

        fs::write(directory.path().join("notes.txt"), "not a manifest").expect("write notes");
        assert!(matches!(discover_manifests(directory.path()), Err(PluginError::Io(_))));
    }
}
