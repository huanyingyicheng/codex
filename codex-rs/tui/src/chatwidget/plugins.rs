use std::collections::HashMap;

use codex_core::config::Config;
use codex_core::git_info::get_git_repo_root;
use codex_core::plugins::PluginManifest;
use codex_core::plugins::PluginRegistry;
use codex_core::plugins::PluginRegistryEntry;
use codex_core::plugins::PluginScope;
use codex_core::plugins::PluginStore;

use crate::bottom_pane::PluginToggleItem;
use crate::bottom_pane::PluginsToggleView;

use super::ChatWidget;

impl ChatWidget {
    pub(crate) fn open_plugins_popup(&mut self) {
        // 关键逻辑：从用户与项目作用域加载插件 registry 并合并展示。
        let mut items = Vec::new();
        for store in plugin_stores(&self.config) {
            match PluginRegistry::load(&store.registry_path()) {
                Ok(registry) => {
                    for entry in registry.plugins {
                        items.push(build_plugin_item(&store, &entry));
                    }
                }
                Err(err) => {
                    let path = store.registry_path();
                    let path_display = path.display();
                    self.add_error_message(format!(
                        "Failed to load plugin registry {path_display}: {err}"
                    ));
                }
            }
        }

        if items.is_empty() {
            self.add_info_message("No plugins installed.".to_string(), None);
            return;
        }

        items.sort_by(|a, b| {
            a.name
                .cmp(&b.name)
                .then_with(|| scope_rank(a.scope.clone()).cmp(&scope_rank(b.scope.clone())))
        });

        let mut initial_state = HashMap::new();
        for item in &items {
            initial_state.insert(plugin_key(&item.name, item.scope.clone()), item.enabled);
        }
        self.plugins_initial_state = Some(initial_state);
        self.plugins_all = items.clone();

        let view = PluginsToggleView::new(items, self.app_event_tx.clone());
        self.bottom_pane.show_view(Box::new(view));
    }

    pub(crate) fn update_plugin_enabled(
        &mut self,
        name: String,
        scope: PluginScope,
        enabled: bool,
    ) {
        for item in &mut self.plugins_all {
            if item.name == name && item.scope == scope {
                item.enabled = enabled;
            }
        }
    }

    pub(crate) fn handle_manage_plugins_closed(&mut self) {
        let Some(initial_state) = self.plugins_initial_state.take() else {
            return;
        };
        let mut current_state = HashMap::new();
        for item in &self.plugins_all {
            current_state.insert(plugin_key(&item.name, item.scope.clone()), item.enabled);
        }

        let mut enabled_count = 0;
        let mut disabled_count = 0;
        for (key, was_enabled) in initial_state {
            let Some(is_enabled) = current_state.get(&key) else {
                continue;
            };
            if was_enabled != *is_enabled {
                if *is_enabled {
                    enabled_count += 1;
                } else {
                    disabled_count += 1;
                }
            }
        }

        if enabled_count == 0 && disabled_count == 0 {
            return;
        }
        self.add_info_message(
            format!("{enabled_count} plugins enabled, {disabled_count} plugins disabled"),
            None,
        );
    }
}

fn plugin_stores(config: &Config) -> Vec<PluginStore> {
    // 关键逻辑：用户与项目作用域同时生效，项目根目录优先使用 git 目录。
    let mut stores = vec![PluginStore::user_scope(&config.codex_home)];
    let root = get_git_repo_root(&config.cwd).unwrap_or_else(|| config.cwd.to_path_buf());
    stores.push(PluginStore::project_scope(&root.join(".codex")));
    stores
}

fn build_plugin_item(store: &PluginStore, entry: &PluginRegistryEntry) -> PluginToggleItem {
    let root = store.plugin_dir(&entry.name);
    let description = match PluginManifest::load_from(&root) {
        Ok(manifest) => manifest.description.unwrap_or_else(|| "-".to_string()),
        Err(_) => "manifest missing".to_string(),
    };

    PluginToggleItem {
        name: entry.name.clone(),
        description,
        enabled: entry.enabled,
        scope: entry.scope.clone(),
        compliance_hint: compliance_hint(entry),
    }
}

fn compliance_hint(entry: &PluginRegistryEntry) -> Option<String> {
    if entry.compliance.warnings.is_empty() {
        return None;
    }
    Some(entry.compliance.warnings.join("; "))
}

fn scope_rank(scope: PluginScope) -> u8 {
    match scope {
        PluginScope::Project => 0,
        PluginScope::User => 1,
    }
}

fn plugin_key(name: &str, scope: PluginScope) -> String {
    let scope_label = match scope {
        PluginScope::User => "user",
        PluginScope::Project => "project",
    };
    format!("{scope_label}:{name}")
}
