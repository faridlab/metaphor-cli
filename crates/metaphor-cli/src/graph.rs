//! Project dependency graph for `metaphor graph` and affected-set computation.
//!
//! Reads the `depends_on` edges from `metaphor_workspace::Manifest` and
//! provides topological ordering, cycle detection, focus subgraphs, and
//! text / JSON rendering.

use anyhow::{bail, Result};
use metaphor_workspace::Manifest;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// A DAG of project names. `edges[name]` lists the names `name` depends on.
#[derive(Debug, Clone)]
pub struct Graph {
    edges: BTreeMap<String, Vec<String>>,
}

#[allow(dead_code)] // nodes/topo_sort land with PLAN.md items #5/#6
impl Graph {
    pub fn from_manifest(manifest: &Manifest) -> Self {
        let mut edges = BTreeMap::new();
        for p in &manifest.projects {
            edges.insert(p.name.clone(), p.depends_on.clone());
        }
        Self { edges }
    }

    pub fn nodes(&self) -> impl Iterator<Item = &str> {
        self.edges.keys().map(String::as_str)
    }

    pub fn deps_of(&self, name: &str) -> &[String] {
        self.edges.get(name).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Kahn's algorithm. Output order has dependencies before their dependents.
    /// Returns `Err` if a cycle is present.
    pub fn topo_sort(&self) -> Result<Vec<String>> {
        // A node's in-degree here = how many deps it still has unsatisfied.
        let mut indegree: BTreeMap<&str, usize> = self
            .edges
            .iter()
            .map(|(k, v)| (k.as_str(), v.len()))
            .collect();
        // FIFO so output within a "ready" layer follows BTreeMap-sorted order
        // (deterministic and stable as projects are added/removed).
        let mut queue: VecDeque<&str> = indegree
            .iter()
            .filter(|(_, &n)| n == 0)
            .map(|(k, _)| *k)
            .collect();
        let mut out = Vec::new();
        while let Some(node) = queue.pop_front() {
            out.push(node.to_string());
            for (parent, deps) in &self.edges {
                if deps.iter().any(|d| d == node) {
                    let e = indegree.get_mut(parent.as_str()).unwrap();
                    *e -= 1;
                    if *e == 0 {
                        queue.push_back(parent.as_str());
                    }
                }
            }
        }
        if out.len() != self.edges.len() {
            let remaining: Vec<String> = self
                .edges
                .keys()
                .filter(|k| !out.contains(k))
                .cloned()
                .collect();
            bail!("cycle detected among projects: {}", remaining.join(", "));
        }
        Ok(out)
    }

    /// Transitive closure of reverse dependencies: every project that would
    /// be impacted by a change to `root`. Used by `--affected`.
    /// The `root` itself is included in the returned set.
    pub fn reverse_deps(&self, root: &str) -> Result<BTreeSet<String>> {
        if !self.edges.contains_key(root) {
            bail!("unknown project '{}'", root);
        }
        let mut out = BTreeSet::new();
        let mut stack = vec![root.to_string()];
        while let Some(n) = stack.pop() {
            if out.insert(n.clone()) {
                // Find every parent whose depends_on contains n.
                for (parent, deps) in &self.edges {
                    if deps.iter().any(|d| d == &n) {
                        stack.push(parent.clone());
                    }
                }
            }
        }
        Ok(out)
    }

    /// Return the subgraph containing `root` and every project reachable from
    /// it via `depends_on` edges.
    pub fn focus(&self, root: &str) -> Result<Graph> {
        if !self.edges.contains_key(root) {
            bail!("unknown project '{}'", root);
        }
        let mut reachable = BTreeSet::new();
        let mut stack = vec![root.to_string()];
        while let Some(n) = stack.pop() {
            if reachable.insert(n.clone()) {
                for d in self.deps_of(&n) {
                    stack.push(d.clone());
                }
            }
        }
        let edges = self
            .edges
            .iter()
            .filter(|(k, _)| reachable.contains(k.as_str()))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        Ok(Graph { edges })
    }

    pub fn render_text(&self) -> String {
        let mut out = String::new();
        for (name, deps) in &self.edges {
            out.push_str(&format!("{name}\n"));
            if let Some((last, rest)) = deps.split_last() {
                for d in rest {
                    out.push_str(&format!("  ├─ {d}\n"));
                }
                out.push_str(&format!("  └─ {last}\n"));
            }
        }
        out
    }

    /// Returns the inner `{ "nodes": [...], "edges": [...] }` payload.
    /// Callers wrap it in the standard envelope via `json_envelope`.
    pub fn to_json_data(&self) -> serde_json::Value {
        let nodes: Vec<_> = self.edges.keys().cloned().collect();
        let edges: Vec<_> = self
            .edges
            .iter()
            .flat_map(|(from, deps)| {
                deps.iter()
                    .map(move |to| serde_json::json!({ "from": from, "to": to }))
            })
            .collect();
        serde_json::json!({ "nodes": nodes, "edges": edges })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use metaphor_workspace::{Project, ProjectType, CURRENT_VERSION};

    fn proj(name: &str, deps: &[&str]) -> Project {
        Project {
            name: name.to_string(),
            project_type: ProjectType::Module,
            path: format!("./{name}"),
            remote: None,
            depends_on: deps.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn manifest(projects: Vec<Project>) -> Manifest {
        Manifest {
            version: CURRENT_VERSION,
            projects,
        }
    }

    #[test]
    fn topo_sort_orders_deps_first() {
        let g = Graph::from_manifest(&manifest(vec![
            proj("web", &["api", "domain"]),
            proj("api", &["domain"]),
            proj("domain", &[]),
        ]));
        let order = g.topo_sort().unwrap();
        let pos = |n| order.iter().position(|x| x == n).unwrap();
        assert!(pos("domain") < pos("api"));
        assert!(pos("api") < pos("web"));
    }

    #[test]
    fn topo_sort_detects_cycle() {
        // manual construction — bypasses Manifest::validate
        let mut edges = BTreeMap::new();
        edges.insert("a".into(), vec!["b".into()]);
        edges.insert("b".into(), vec!["a".into()]);
        let g = Graph { edges };
        assert!(g.topo_sort().is_err());
    }

    #[test]
    fn focus_returns_reachable_subgraph() {
        let g = Graph::from_manifest(&manifest(vec![
            proj("web", &["api"]),
            proj("api", &["domain"]),
            proj("domain", &[]),
            proj("unrelated", &[]),
        ]));
        let sub = g.focus("api").unwrap();
        let names: Vec<_> = sub.nodes().collect();
        assert_eq!(names, vec!["api", "domain"]);
    }

    #[test]
    fn focus_rejects_unknown() {
        let g = Graph::from_manifest(&manifest(vec![proj("a", &[])]));
        assert!(g.focus("ghost").is_err());
    }

    #[test]
    fn reverse_deps_transitive_closure() {
        // web → api → domain; changing domain affects {domain, api, web}.
        let g = Graph::from_manifest(&manifest(vec![
            proj("web", &["api"]),
            proj("api", &["domain"]),
            proj("domain", &[]),
            proj("unrelated", &[]),
        ]));
        let affected = g.reverse_deps("domain").unwrap();
        let names: Vec<_> = affected.into_iter().collect();
        assert_eq!(names, vec!["api", "domain", "web"]);
    }

    #[test]
    fn reverse_deps_rejects_unknown() {
        let g = Graph::from_manifest(&manifest(vec![proj("a", &[])]));
        assert!(g.reverse_deps("ghost").is_err());
    }

    #[test]
    fn to_json_data_shape_is_stable() {
        let g = Graph::from_manifest(&manifest(vec![proj("a", &[]), proj("b", &["a"])]));
        let v = g.to_json_data();
        // Nodes sorted by BTreeMap; edge-list reflects `depends_on`.
        assert_eq!(v["nodes"], serde_json::json!(["a", "b"]));
        assert_eq!(v["edges"], serde_json::json!([{ "from": "b", "to": "a" }]));
    }
}
