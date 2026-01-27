pub mod installer;
pub mod manifest;
pub mod policy;
pub mod registry;
pub mod validator;

use std::collections::HashSet;
use std::path::PathBuf;

use tracing::warn;

use crate::config::Config;
use crate::config_loader::ConfigLayerStack;
use crate::config_loader::ConfigLayerStackOrdering;
use codex_app_server_protocol::ConfigLayerSource;

pub use installer::InstallOutcome;
pub use installer::PluginInstaller;
pub use installer::PluginMarketplaceEntry;
pub use installer::PluginMarketplaceIndex;
pub use installer::PluginStore;
pub use manifest::PluginAuthor;
pub use manifest::PluginComponent;
pub use manifest::PluginManifest;
pub use policy::PluginPolicy;
pub use registry::PluginComplianceReport;
pub use registry::PluginRegistry;
pub use registry::PluginRegistryEntry;
pub use registry::PluginRuntimeSpec;
pub use registry::PluginScope;
pub use registry::PluginSource;

pub trait PluginRuntime: Send + Sync {
    fn name(&self) -> &str;
}

#[derive(Debug, Clone)]
pub struct InstalledPlugin {
    pub entry: PluginRegistryEntry,
    pub manifest: PluginManifest,
    pub root: PathBuf,
}

#[derive(Debug, Clone)]
pub struct PluginComponentDir {
    pub plugin_name: String,
    pub scope: PluginScope,
    pub path: PathBuf,
}

pub fn plugin_stores_from_config(
    config: &Config,
    ordering: ConfigLayerStackOrdering,
) -> Vec<PluginStore> {
    plugin_stores_from_layer_stack(&config.config_layer_stack, ordering)
}

pub fn plugin_stores_from_layer_stack(
    config_layer_stack: &ConfigLayerStack,
    ordering: ConfigLayerStackOrdering,
) -> Vec<PluginStore> {
    let mut seen = HashSet::new();
    let mut stores = Vec::new();

    for layer in config_layer_stack.get_layers(ordering, true) {
        let Some(config_folder) = layer.config_folder() else {
            continue;
        };
        let store = match &layer.name {
            ConfigLayerSource::Project { .. } => {
                PluginStore::project_scope(config_folder.as_path())
            }
            ConfigLayerSource::User { .. } => PluginStore::user_scope(config_folder.as_path()),
            ConfigLayerSource::System { .. }
            | ConfigLayerSource::Mdm { .. }
            | ConfigLayerSource::SessionFlags
            | ConfigLayerSource::LegacyManagedConfigTomlFromFile { .. }
            | ConfigLayerSource::LegacyManagedConfigTomlFromMdm => {
                continue;
            }
        };

        if seen.insert(store.root().to_path_buf()) {
            stores.push(store);
        }
    }

    stores
}

pub fn load_enabled_plugins(stores: &[PluginStore]) -> Vec<InstalledPlugin> {
    let mut plugins = Vec::new();
    for store in stores {
        let registry = match PluginRegistry::load(&store.registry_path()) {
            Ok(registry) => registry,
            Err(err) => {
                warn!(
                    "failed to load plugin registry {}: {err:#}",
                    store.registry_path().display()
                );
                continue;
            }
        };
        for entry in registry.plugins.iter().filter(|entry| entry.enabled) {
            let root = store.plugin_dir(&entry.name);
            if !root.exists() {
                warn!(
                    "plugin {} missing on disk at {}",
                    entry.name,
                    root.display()
                );
                continue;
            }
            let manifest = match PluginManifest::load_from(&root) {
                Ok(manifest) => manifest,
                Err(err) => {
                    warn!("failed to load plugin manifest {}: {err:#}", root.display());
                    continue;
                }
            };
            plugins.push(InstalledPlugin {
                entry: entry.clone(),
                manifest,
                root,
            });
        }
    }
    plugins
}

pub fn plugin_component_dirs_from_stores(
    stores: &[PluginStore],
    component: PluginComponent,
) -> Vec<PluginComponentDir> {
    let plugins = load_enabled_plugins(stores);
    let mut out = Vec::new();
    for plugin in plugins {
        if component == PluginComponent::Hooks && !plugin.entry.policy.allow_hooks {
            continue;
        }
        let Some(path) = plugin
            .manifest
            .resolve_component_dir(&plugin.root, component)
        else {
            continue;
        };
        let is_valid = if component == PluginComponent::McpConfigs {
            path.is_dir() || path.is_file()
        } else {
            path.is_dir()
        };
        if !is_valid {
            continue;
        }
        out.push(PluginComponentDir {
            plugin_name: plugin.entry.name.clone(),
            scope: plugin.entry.scope,
            path,
        });
    }
    out
}
