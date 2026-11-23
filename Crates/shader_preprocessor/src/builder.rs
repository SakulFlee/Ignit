use std::collections::HashSet;
use std::ffi::OsStr;
use std::sync::{Arc, OnceLock};

use crate::{NodeRegistry, ShaderNode};

/// Environment variable that opts into printing the assembled WGSL source.
///
/// Any value other than an empty string or `0` enables it. The setting is read
/// once per process, on the first shader build.
pub const LOG_SHADER_SOURCE_ENV: &str = "IGNIT_LOG_SHADER_SOURCE";

fn env_flag_enabled(value: Option<&OsStr>) -> bool {
    value.is_some_and(|value| !value.is_empty() && value != OsStr::new("0"))
}

fn log_shader_source_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| env_flag_enabled(std::env::var_os(LOG_SHADER_SOURCE_ENV).as_deref()))
}

/// Assembles a full WGSL shader from registered [`ShaderNode`]s.
///
/// Nodes are requested **by name** and resolved against a [`NodeRegistry`].
/// Dependencies are resolved transitively, each node is emitted exactly once,
/// and dependencies always precede their dependents. Additional raw WGSL
/// (e.g. an entrypoint body) can be appended after the nodes.
///
/// This replaces the old runtime directory-listing `#import` approach: all
/// node source is code-defined (via `include_str!` or runtime strings), so
/// shader assembly is fully portable (including Android).
#[derive(Debug)]
pub struct ShaderBuilder {
    registry: Arc<NodeRegistry>,
    /// Ordered, deduplicated node names resolved so far.
    order: Vec<Arc<str>>,
    /// Extra raw WGSL appended after the resolved nodes.
    extra: Vec<Arc<str>>,
    /// Names of nodes currently being visited on the resolution stack, used to
    /// detect dependency cycles.
    visiting: HashSet<Arc<str>>,
    /// When enabled, `build()` logs the ordered node list and full assembled
    /// WGSL source via `log::debug!`. Off by default.
    debug: bool,
}

impl ShaderBuilder {
    /// Creates a builder resolving nodes against the given registry.
    pub fn new(registry: Arc<NodeRegistry>) -> Self {
        Self {
            registry,
            order: Vec::new(),
            extra: Vec::new(),
            visiting: HashSet::new(),
            debug: false,
        }
    }

    /// Creates a builder resolving nodes against the process-wide global
    /// registry (preloaded with the engine prelude).
    pub fn with_global_registry() -> Self {
        Self::new(Arc::new(NodeRegistry::global().clone()))
    }

    /// Enables (or disables) `log::debug!` output of the assembled node list
    /// and full WGSL source when [`ShaderBuilder::build`] is called.
    ///
    /// Logging is off by default. Setting the [`LOG_SHADER_SOURCE_ENV`]
    /// environment variable to a non-empty value other than `0` also enables
    /// it for every builder in the process, without recompiling.
    pub fn set_debug(&mut self, debug: bool) -> &mut Self {
        self.debug = debug;
        self
    }

    /// Whether debug output is enabled.
    pub fn debug(&self) -> bool {
        self.debug
    }

    /// Requests a node by name, resolving its transitive dependencies
    /// (depth-first, each emitted once, dependencies before dependents).
    ///
    /// Returns [`crate::ShaderPreprocessorError::UnknownNode`] if the name is
    /// not registered, or
    /// [`crate::ShaderPreprocessorError::DependencyCycle`] if resolving the
    /// node would follow a circular dependency chain.
    pub fn add_node(&mut self, name: &str) -> Result<&mut Self, crate::ShaderPreprocessorError> {
        let node =
            self.registry
                .get(name)
                .ok_or_else(|| crate::ShaderPreprocessorError::UnknownNode {
                    name: name.to_string(),
                })?;
        self.resolve(&node)?;
        Ok(self)
    }

    /// Requests several nodes by name.
    pub fn add_nodes(
        &mut self,
        names: &[&str],
    ) -> Result<&mut Self, crate::ShaderPreprocessorError> {
        for name in names {
            self.add_node(name)?;
        }
        Ok(self)
    }

    /// Appends raw WGSL after the resolved nodes (e.g. an entrypoint body that
    /// is not itself a reusable node).
    pub fn add_source(&mut self, source: impl Into<Arc<str>>) -> &mut Self {
        self.extra.push(source.into());
        self
    }

    /// Recursively resolves a node and its dependencies into `self.order`,
    /// emitting each node exactly once.
    ///
    /// `visiting` tracks nodes on the current DFS path so that circular
    /// dependencies are reported as an error instead of recursing forever.
    fn resolve(&mut self, node: &Arc<ShaderNode>) -> Result<(), crate::ShaderPreprocessorError> {
        // Guard against cycles: if this node is already on the resolution stack
        // we have a circular dependency.
        if self.visiting.contains(&node.name) {
            return Err(crate::ShaderPreprocessorError::DependencyCycle {
                name: node.name.to_string(),
            });
        }

        // Already fully resolved (and emitted) on a previous path.
        if self.order.iter().any(|o| *o == node.name) {
            return Ok(());
        }

        self.visiting.insert(node.name.clone());

        let deps = node.depends_on.clone();
        for dep in deps {
            if self.order.contains(&dep) {
                continue;
            }
            if let Some(dep_node) = self.registry.get(&dep) {
                self.resolve(&dep_node)?;
            }
        }

        self.visiting.remove(&node.name);
        self.order.push(node.name.clone());
        Ok(())
    }

    /// Emits the assembled WGSL source: the resolved nodes (dependency-first,
    /// deduplicated) followed by any raw source appended via
    /// [`ShaderBuilder::add_source`].
    ///
    /// If debug output is enabled (see [`ShaderBuilder::set_debug`] and
    /// [`LOG_SHADER_SOURCE_ENV`]), the ordered node list and full assembled
    /// source are logged via `log::debug!`.
    pub fn build(&self) -> String {
        let mut out = String::new();
        for name in &self.order {
            if let Some(node) = self.registry.get(name) {
                if !out.is_empty() {
                    out.push('\n');
                }
                out.push_str(&node.source);
            }
        }
        for extra in &self.extra {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(extra);
        }

        if self.debug || log_shader_source_enabled() {
            let nodes = self
                .order
                .iter()
                .map(|n| n.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            log::debug!(
                "[ShaderBuilder] resolved nodes ({}): {nodes}",
                self.order.len()
            );
            log::debug!("[ShaderBuilder] assembled WGSL:\n{out}");
        }

        out
    }

    /// Returns the ordered, deduplicated list of node names that will be
    /// emitted by [`ShaderBuilder::build`]. Useful for diagnostics and the
    /// future node-graph visualization.
    pub fn resolved_order(&self) -> &[Arc<str>] {
        &self.order
    }

    /// A deduplicated set of node names (for graph introspection).
    pub fn node_names(&self) -> HashSet<&str> {
        self.order.iter().map(|s| &**s).collect()
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;

    use super::env_flag_enabled;

    #[test]
    fn shader_logging_env_flag_is_off_unless_explicitly_enabled() {
        assert!(!env_flag_enabled(None));
        assert!(!env_flag_enabled(Some(OsStr::new(""))));
        assert!(!env_flag_enabled(Some(OsStr::new("0"))));
        assert!(env_flag_enabled(Some(OsStr::new("1"))));
        assert!(env_flag_enabled(Some(OsStr::new("true"))));
    }
}
