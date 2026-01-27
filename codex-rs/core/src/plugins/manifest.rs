use serde::Deserialize;
use serde::Serialize;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct PluginAuthor {
    pub name: String,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginComponent {
    Commands,
    Skills,
    Rules,
    Contexts,
    Hooks,
    Agents,
    McpConfigs,
}

impl PluginComponent {
    pub fn default_dir_name(self) -> &'static str {
        match self {
            PluginComponent::Commands => "commands",
            PluginComponent::Skills => "skills",
            PluginComponent::Rules => "rules",
            PluginComponent::Contexts => "contexts",
            PluginComponent::Hooks => "hooks",
            PluginComponent::Agents => "agents",
            PluginComponent::McpConfigs => "mcp-configs",
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct PluginManifest {
    pub name: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub author: Option<PluginAuthor>,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default)]
    pub repository: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub commands: Option<PathBuf>,
    #[serde(default)]
    pub skills: Option<PathBuf>,
    #[serde(default)]
    pub rules: Option<PathBuf>,
    #[serde(default)]
    pub contexts: Option<PathBuf>,
    #[serde(default)]
    pub hooks: Option<PathBuf>,
    #[serde(default)]
    pub agents: Option<PathBuf>,
    #[serde(default, rename = "mcp-configs", alias = "mcpServers")]
    pub mcp_configs: Option<PathBuf>,
}

impl PluginManifest {
    pub fn load_from(root: &Path) -> anyhow::Result<Self> {
        let claude_path = root.join(".claude-plugin").join("plugin.json");
        let root_path = root.join("plugin.json");
        let path = if claude_path.exists() {
            claude_path
        } else {
            root_path
        };
        let data = std::fs::read_to_string(&path)?;
        let manifest = serde_json::from_str::<PluginManifest>(&data)?;
        Ok(manifest)
    }

    pub fn component_path(&self, component: PluginComponent) -> Option<&PathBuf> {
        match component {
            PluginComponent::Commands => self.commands.as_ref(),
            PluginComponent::Skills => self.skills.as_ref(),
            PluginComponent::Rules => self.rules.as_ref(),
            PluginComponent::Contexts => self.contexts.as_ref(),
            PluginComponent::Hooks => self.hooks.as_ref(),
            PluginComponent::Agents => self.agents.as_ref(),
            PluginComponent::McpConfigs => self.mcp_configs.as_ref(),
        }
    }

    pub fn resolve_component_dir(
        &self,
        root: &Path,
        component: PluginComponent,
    ) -> Option<PathBuf> {
        // 关键逻辑：优先使用 manifest 路径，否则回退到约定目录名（如 commands/skills）。
        let relative = if let Some(path) = self.component_path(component) {
            if path.is_absolute()
                || path
                    .components()
                    .any(|part| matches!(part, Component::ParentDir))
            {
                return None;
            }
            path.to_path_buf()
        } else {
            let fallback = root.join(component.default_dir_name());
            if !fallback.exists() {
                return None;
            }
            PathBuf::from(component.default_dir_name())
        };

        let candidate = root.join(&relative);
        if !candidate.exists() {
            return None;
        }

        let canonical_root = dunce::canonicalize(root).ok()?;
        let canonical_candidate = dunce::canonicalize(candidate).ok()?;
        // 关键逻辑：canonicalize 后仍需确保路径在插件根目录内。
        canonical_candidate
            .starts_with(&canonical_root)
            .then_some(canonical_candidate)
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use tempfile::tempdir;

    use super::PluginManifest;

    #[test]
    fn loads_manifest_from_claude_plugin_path() -> anyhow::Result<()> {
        let tmp = tempdir()?;
        let root = tmp.path().join("demo");
        std::fs::create_dir_all(root.join(".claude-plugin"))?;
        std::fs::write(
            root.join(".claude-plugin").join("plugin.json"),
            r#"{"name":"demo","commands":"./commands"}"#,
        )?;

        let manifest = PluginManifest::load_from(&root)?;
        assert_eq!(manifest.name, "demo");
        assert_eq!(
            manifest.commands.as_deref(),
            Some(std::path::Path::new("./commands"))
        );
        Ok(())
    }

    #[test]
    fn falls_back_to_root_plugin_json() -> anyhow::Result<()> {
        let tmp = tempdir()?;
        let root = tmp.path().join("demo");
        std::fs::create_dir_all(&root)?;
        std::fs::write(root.join("plugin.json"), r#"{"name":"demo"}"#)?;

        let manifest = PluginManifest::load_from(&root)?;
        assert_eq!(manifest.name, "demo");
        Ok(())
    }
}
