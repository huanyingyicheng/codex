use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, Default)]
pub struct PluginPolicy {
    #[serde(default)]
    pub allow_hooks: bool,
    #[serde(default)]
    pub allow_scripts: bool,
}
