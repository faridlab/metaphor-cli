//! Built-in plugins compiled into metaphor-cli.
//!
//! Each plugin wraps an external backbone tool as a subprocess. They are
//! statically registered by [`all_plugins`] and dispatched by consumer type
//! in [`crate::commands::generate`].

pub mod backbone_mobilegen;
pub mod backbone_schema;

use metaphor_plugin_api::GeneratorPlugin;

/// All plugins compiled into this build of metaphor-cli.
pub fn all_plugins() -> Vec<Box<dyn GeneratorPlugin>> {
    vec![
        Box::new(backbone_schema::BackboneSchemaPlugin),
        Box::new(backbone_mobilegen::BackboneMobilegenPlugin),
    ]
}
