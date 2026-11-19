//! Enforces the hexagonal layering through the dependency graph.
//!
//! - `domain`, `ports` and `application` never reach an I/O crate, even
//!   transitively.
//! - Among the workspace crates, `domain` depends on none, `ports` only on
//!   `domain`, `application` only on `domain` and `ports`.
//!
//! Only normal and build dependencies count: a dev-dependency used by a
//! test does not ship in the crate.

use std::collections::{BTreeSet, HashMap, VecDeque};

use cargo_metadata::{DependencyKind, Metadata, MetadataCommand, Node, PackageId};

/// Crates that perform I/O or pull in a runtime. Extend the list when an
/// adapter brings a new one.
const IO_CRATES: &[&str] = &[
    "async-imap",
    "async-std",
    "axum",
    "hyper",
    "imap",
    "lettre",
    "mio",
    "reqwest",
    "rusqlite",
    "smol",
    "sqlx",
    "tokio",
    "tower",
    "ureq",
];

/// Workspace crates each core crate may depend on.
const ALLOWED_WORKSPACE_DEPS: &[(&str, &[&str])] = &[
    ("domain", &[]),
    ("ports", &["domain"]),
    ("application", &["domain", "ports"]),
];

fn metadata() -> Metadata {
    MetadataCommand::new()
        .manifest_path(concat!(env!("CARGO_MANIFEST_DIR"), "/../../Cargo.toml"))
        .exec()
        .expect("cargo metadata")
}

struct Graph<'a> {
    nodes: HashMap<&'a PackageId, &'a Node>,
    names: HashMap<&'a PackageId, &'a str>,
}

impl<'a> Graph<'a> {
    fn new(metadata: &'a Metadata) -> Self {
        let resolve = metadata.resolve.as_ref().expect("resolved graph");
        Self {
            nodes: resolve.nodes.iter().map(|n| (&n.id, n)).collect(),
            names: metadata
                .packages
                .iter()
                .map(|p| (&p.id, p.name.as_str()))
                .collect(),
        }
    }

    /// Direct dependencies that ship with the crate (normal or build).
    fn shipped_deps(&self, id: &'a PackageId) -> impl Iterator<Item = &'a PackageId> + '_ {
        self.nodes[id].deps.iter().filter_map(|dep| {
            dep.dep_kinds
                .iter()
                .any(|k| k.kind != DependencyKind::Development)
                .then_some(&dep.pkg)
        })
    }

    /// Every crate `root` reaches through shipped dependencies.
    fn closure(&self, root: &'a PackageId) -> BTreeSet<&'a str> {
        let mut seen = BTreeSet::new();
        let mut queue = VecDeque::from([root]);
        let mut visited = BTreeSet::from([root]);
        while let Some(id) = queue.pop_front() {
            for dep in self.shipped_deps(id) {
                if visited.insert(dep) {
                    seen.insert(self.names[dep]);
                    queue.push_back(dep);
                }
            }
        }
        seen
    }
}

fn workspace_package<'a>(metadata: &'a Metadata, name: &str) -> &'a PackageId {
    &metadata
        .workspace_packages()
        .into_iter()
        .find(|p| p.name.as_str() == name)
        .unwrap_or_else(|| panic!("workspace crate `{name}` not found"))
        .id
}

#[test]
fn core_crates_reach_no_io_crate() {
    let metadata = metadata();
    let graph = Graph::new(&metadata);
    for (name, _) in ALLOWED_WORKSPACE_DEPS {
        let closure = graph.closure(workspace_package(&metadata, name));
        let io: Vec<_> = IO_CRATES.iter().filter(|c| closure.contains(*c)).collect();
        assert!(io.is_empty(), "`{name}` depends on I/O crates: {io:?}");
    }
}

#[test]
fn core_crates_depend_only_on_inner_layers() {
    let metadata = metadata();
    let graph = Graph::new(&metadata);
    let workspace: BTreeSet<&str> = metadata
        .workspace_packages()
        .iter()
        .map(|p| p.name.as_str())
        .collect();
    for (name, allowed) in ALLOWED_WORKSPACE_DEPS {
        let id = workspace_package(&metadata, name);
        let forbidden: Vec<_> = graph
            .shipped_deps(id)
            .map(|dep| graph.names[dep])
            .filter(|dep| workspace.contains(dep) && !allowed.contains(dep))
            .collect();
        assert!(
            forbidden.is_empty(),
            "`{name}` may depend only on {allowed:?}, found {forbidden:?}"
        );
    }
}
