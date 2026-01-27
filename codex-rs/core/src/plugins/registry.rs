use serde::Deserialize;
use serde::Serialize;
use std::path::Path;

use crate::path_utils::write_atomically;
use crate::plugins::policy::PluginPolicy;

const REGISTRY_VERSION: u32 = 1;

fn default_registry_version() -> u32 {
    REGISTRY_VERSION
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PluginScope {
    User,
    Project,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PluginSource {
    LocalPath(String),
    Url(String),
    GitHub {
        repo: String,
        reference: Option<String>,
    },
    Marketplace {
        name: String,
        source: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginComplianceReport {
    #[serde(default)]
    pub errors: Vec<String>,
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(default)]
    pub hooks_detected: bool,
    #[serde(default)]
    pub scripts_detected: bool,
}

impl PluginComplianceReport {
    pub fn empty() -> Self {
        Self {
            errors: Vec::new(),
            warnings: Vec::new(),
            hooks_detected: false,
            scripts_detected: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginRuntimeSpec {
    pub kind: String,
    pub entrypoint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginRegistryEntry {
    pub name: String,
    pub enabled: bool,
    pub scope: PluginScope,
    pub source: PluginSource,
    pub policy: PluginPolicy,
    pub compliance: PluginComplianceReport,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime: Option<PluginRuntimeSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginRegistry {
    #[serde(default = "default_registry_version")]
    pub version: u32,
    #[serde(default)]
    pub plugins: Vec<PluginRegistryEntry>,
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self {
            version: REGISTRY_VERSION,
            plugins: Vec::new(),
        }
    }
}

impl PluginRegistry {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = std::fs::read_to_string(path)?;
        Ok(serde_json::from_str(&text)?)
    }

    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        let text = serde_json::to_string_pretty(self)?;
        write_atomically(path, &text)?;
        Ok(())
    }

    pub fn find_entry(&self, name: &str) -> Option<&PluginRegistryEntry> {
        self.plugins.iter().find(|entry| entry.name == name)
    }

    pub fn find_entry_mut(&mut self, name: &str) -> Option<&mut PluginRegistryEntry> {
        self.plugins.iter_mut().find(|entry| entry.name == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use tempfile::tempdir;

    #[test]
    fn registry_round_trips() -> anyhow::Result<()> {
        let tmp = tempdir()?;
        let path = tmp.path().join("installed_plugins.json");
        let mut registry = PluginRegistry::default();
        registry.plugins.push(PluginRegistryEntry {
            name: "demo".to_string(),
            enabled: true,
            scope: PluginScope::User,
            source: PluginSource::LocalPath("C:/demo".to_string()),
            policy: PluginPolicy {
                allow_hooks: false,
                allow_scripts: false,
            },
            compliance: PluginComplianceReport::empty(),
            runtime: None,
        });

        registry.save(&path)?;
        let loaded = PluginRegistry::load(&path)?;
        assert_eq!(loaded, registry);
        Ok(())
    }
}
