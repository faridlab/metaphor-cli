//! Built-in plugins compiled into metaphor-cli.
//!
//! Each plugin wraps an external metaphor-plugin-* tool as a subprocess.
//! They are statically registered by [`all_plugins`] and dispatched by the
//! `(producer_type, consumer_type)` pair.

pub mod schema;
pub mod webgen;

use metaphor_plugin_api::GeneratorPlugin;

/// All plugins compiled into this build of metaphor-cli.
///
/// Note: there is no separate `mobilegen` plugin. Mobile codegen was merged
/// into `metaphor-plugin-schema` (the `kotlin` subcommand) so the `SchemaPlugin`
/// here handles both `Module → Module` (server-side) and `Module → Mobileapp`
/// (Kotlin) by dispatching to the right subcommand of the same binary.
pub fn all_plugins() -> Vec<Box<dyn GeneratorPlugin>> {
    vec![
        Box::new(schema::SchemaPlugin),
        Box::new(webgen::WebgenPlugin),
    ]
}
