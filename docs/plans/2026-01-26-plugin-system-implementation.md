# Plugin System Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Implement installable content plugins (commands/skills/rules/contexts/hooks/agents/mcp-configs) with compliance checks, CLI/TUI management, and a clear extension point for runtime plugins (C).

**Architecture:** Add a `plugins` module in `codex-core` to parse manifests, validate content, manage registries, and install plugins. Integrate plugin component roots into prompts, skills, rules, contexts, and MCP server loading. Expose management via `codex plugin` CLI and a `/plugins` TUI view.

**Tech Stack:** Rust (codex-core/cli/tui), serde, tokio, reqwest, zip, ratatui.

---

### Task 1: Core plugin models and registry persistence

**Files:**
- Create: `codex-rs/core/src/plugins/mod.rs`
- Create: `codex-rs/core/src/plugins/manifest.rs`
- Create: `codex-rs/core/src/plugins/registry.rs`
- Create: `codex-rs/core/src/plugins/policy.rs`
- Modify: `codex-rs/core/src/lib.rs`
- Test: `codex-rs/core/src/plugins/manifest.rs`
- Test: `codex-rs/core/src/plugins/registry.rs`

**Step 1: Write failing manifest tests**

```rust
use pretty_assertions::assert_eq;
use tempfile::tempdir;

#[test]
fn loads_manifest_from_claude_plugin_path() -> anyhow::Result<()> {
    let tmp = tempdir()?;
    let root = tmp.path().join("demo");
    std::fs::create_dir_all(root.join(".claude-plugin"))?;
    std::fs::write(
        root.join(".claude-plugin").join("plugin.json"),
        r#"{\"name\":\"demo\",\"commands\":\"./commands\"}"#,
    )?;

    let manifest = PluginManifest::load_from(&root)?;
    assert_eq!(manifest.name, "demo");
    assert_eq!(manifest.commands.as_deref(), Some(std::path::Path::new("./commands")));
    Ok(())
}

#[test]
fn falls_back_to_root_plugin_json() -> anyhow::Result<()> {
    let tmp = tempdir()?;
    let root = tmp.path().join("demo");
    std::fs::create_dir_all(&root)?;
    std::fs::write(root.join("plugin.json"), r#"{\"name\":\"demo\"}"#)?;

    let manifest = PluginManifest::load_from(&root)?;
    assert_eq!(manifest.name, "demo");
    Ok(())
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p codex-core plugins::manifest`
Expected: FAIL with missing `PluginManifest` / `load_from`.

**Step 3: Implement manifest + policy + registry models**

```rust
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, PartialEq)]
pub struct PluginManifest {
    pub name: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub commands: Option<std::path::PathBuf>,
    #[serde(default)]
    pub skills: Option<std::path::PathBuf>,
    #[serde(default)]
    pub rules: Option<std::path::PathBuf>,
    #[serde(default)]
    pub contexts: Option<std::path::PathBuf>,
    #[serde(default)]
    pub hooks: Option<std::path::PathBuf>,
    #[serde(default)]
    pub agents: Option<std::path::PathBuf>,
    #[serde(default, rename = "mcp-configs")]
    pub mcp_configs: Option<std::path::PathBuf>,
}

impl PluginManifest {
    pub fn load_from(root: &std::path::Path) -> anyhow::Result<Self> {
        let claude_path = root.join(".claude-plugin").join("plugin.json");
        let root_path = root.join("plugin.json");
        let path = if claude_path.exists() { claude_path } else { root_path };
        let data = std::fs::read_to_string(&path)?;
        let manifest = serde_json::from_str::<PluginManifest>(&data)?;
        Ok(manifest)
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct PluginPolicy {
    #[serde(default)]
    pub allow_hooks: bool,
    #[serde(default)]
    pub allow_scripts: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct PluginRegistry {
    #[serde(default = "default_registry_version")]
    pub version: u32,
    #[serde(default)]
    pub plugins: Vec<PluginRegistryEntry>,
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test -p codex-core plugins::manifest`
Expected: PASS.

**Step 5: Write failing registry round-trip test**

```rust
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
        policy: PluginPolicy { allow_hooks: false, allow_scripts: false },
        compliance: PluginComplianceReport::empty(),
    });

    registry.save(&path)?;
    let loaded = PluginRegistry::load(&path)?;
    assert_eq!(loaded, registry);
    Ok(())
}
```

**Step 6: Run test to verify it fails**

Run: `cargo test -p codex-core plugins::registry`
Expected: FAIL with missing `save/load`.

**Step 7: Implement registry load/save helpers**

```rust
impl PluginRegistry {
    pub fn load(path: &std::path::Path) -> anyhow::Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = std::fs::read_to_string(path)?;
        Ok(serde_json::from_str(&text)?)
    }

    pub fn save(&self, path: &std::path::Path) -> anyhow::Result<()> {
        let text = serde_json::to_string_pretty(self)?;
        crate::path_utils::write_atomically(path, &text)?;
        Ok(())
    }
}
```

**Step 8: Run test to verify it passes**

Run: `cargo test -p codex-core plugins::registry`
Expected: PASS.

**Step 9: Commit**

```bash
git add codex-rs/core/src/plugins/ codex-rs/core/src/lib.rs

git commit -m "feat(core): add plugin manifest and registry models"
```

---

### Task 2: Validation + safe install (path safety, hook/script scan, zip)

**Files:**
- Modify: `codex-rs/core/src/plugins/mod.rs`
- Create: `codex-rs/core/src/plugins/validator.rs`
- Create: `codex-rs/core/src/plugins/installer.rs`
- Modify: `codex-rs/core/Cargo.toml`
- Test: `codex-rs/core/src/plugins/validator.rs`
- Test: `codex-rs/core/src/plugins/installer.rs`

**Step 1: Write failing validator tests**

```rust
use pretty_assertions::assert_eq;
use tempfile::tempdir;

#[test]
fn rejects_traversal_paths() -> anyhow::Result<()> {
    let tmp = tempdir()?;
    let root = tmp.path().join("demo");
    std::fs::create_dir_all(&root)?;
    std::fs::write(root.join("plugin.json"), r#"{\"name\":\"demo\",\"commands\":\"../evil\"}"#)?;

    let manifest = PluginManifest::load_from(&root)?;
    let report = PluginValidator::validate(&root, &manifest)?;

    assert_eq!(report.errors.len(), 1);
    Ok(())
}

#[test]
fn warns_on_hooks_and_scripts() -> anyhow::Result<()> {
    let tmp = tempdir()?;
    let root = tmp.path().join("demo");
    std::fs::create_dir_all(root.join("hooks"))?;
    std::fs::create_dir_all(root.join("scripts"))?;
    std::fs::write(root.join("plugin.json"), r#"{\"name\":\"demo\",\"hooks\":\"./hooks\"}"#)?;

    let manifest = PluginManifest::load_from(&root)?;
    let report = PluginValidator::validate(&root, &manifest)?;

    assert_eq!(report.warnings.len(), 1);
    assert!(report.hooks_detected);
    assert!(report.scripts_detected);
    Ok(())
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p codex-core plugins::validator`
Expected: FAIL with missing `PluginValidator`.

**Step 3: Implement validator + compliance report**

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct PluginComplianceReport {
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub hooks_detected: bool,
    pub scripts_detected: bool,
}

impl PluginComplianceReport {
    pub fn empty() -> Self {
        Self { errors: Vec::new(), warnings: Vec::new(), hooks_detected: false, scripts_detected: false }
    }
}

pub struct PluginValidator;

impl PluginValidator {
    pub fn validate(root: &std::path::Path, manifest: &PluginManifest) -> anyhow::Result<PluginComplianceReport> {
        let mut report = PluginComplianceReport::empty();
        for path in [
            manifest.commands.as_ref(),
            manifest.skills.as_ref(),
            manifest.rules.as_ref(),
            manifest.contexts.as_ref(),
            manifest.hooks.as_ref(),
            manifest.agents.as_ref(),
            manifest.mcp_configs.as_ref(),
        ].into_iter().flatten() {
            if path.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
                report.errors.push(format!("path escapes plugin root: {}", path.display()));
                continue;
            }
            let candidate = root.join(path);
            let canon = dunce::canonicalize(&candidate)?;
            if !canon.starts_with(root) {
                report.errors.push(format!("path escapes plugin root: {}", path.display()));
            }
        }

        let hooks_dir = manifest.hooks.as_ref().map(|p| root.join(p));
        if hooks_dir.as_ref().is_some_and(|p| p.exists()) {
            report.hooks_detected = true;
            report.warnings.push("hooks detected; policy required".to_string());
        }
        let scripts_dir = root.join("scripts");
        if scripts_dir.exists() {
            report.scripts_detected = true;
            report.warnings.push("scripts detected; policy required".to_string());
        }
        Ok(report)
    }
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test -p codex-core plugins::validator`
Expected: PASS.

**Step 5: Write failing installer test (local path)**

```rust
#[tokio::test]
async fn installs_local_plugin_and_updates_registry() -> anyhow::Result<()> {
    let tmp = tempfile::tempdir()?;
    let src = tmp.path().join("src");
    std::fs::create_dir_all(&src)?;
    std::fs::write(src.join("plugin.json"), r#"{\"name\":\"demo\"}"#)?;

    let store = PluginStore::user_scope(tmp.path());
    let result = PluginInstaller::install_from_path(&store, &src).await?;

    assert_eq!(result.entry.name, "demo");
    let registry = PluginRegistry::load(&store.registry_path())?;
    assert_eq!(registry.plugins.len(), 1);
    Ok(())
}
```

**Step 6: Run test to verify it fails**

Run: `cargo test -p codex-core plugins::installer`
Expected: FAIL with missing installer.

**Step 7: Implement installer + safe extraction**

```rust
pub struct PluginStore { /* root, registry_path */ }

pub struct PluginInstaller;

impl PluginInstaller {
    pub async fn install_from_path(store: &PluginStore, source: &std::path::Path) -> anyhow::Result<InstallOutcome> {
        // 校验路径与权限（中文注释）
        // 1) 读取 manifest 2) validate 3) 复制到目标目录 4) 更新 registry
        Ok(outcome)
    }
}
```

**Step 8: Run test to verify it passes**

Run: `cargo test -p codex-core plugins::installer`
Expected: PASS.

**Step 9: Commit**

```bash
git add codex-rs/core/src/plugins/ codex-rs/core/Cargo.toml

git commit -m "feat(core): validate and install plugins"
```

---

### Task 3: Integrate plugin components (prompts, skills, rules, contexts, mcp)

**Files:**
- Modify: `codex-rs/core/src/custom_prompts.rs`
- Modify: `codex-rs/core/src/codex.rs`
- Modify: `codex-rs/core/src/skills/loader.rs`
- Modify: `codex-rs/core/src/skills/manager.rs`
- Modify: `codex-rs/core/src/exec_policy.rs`
- Modify: `codex-rs/core/src/project_doc.rs`
- Modify: `codex-rs/core/src/mcp/mod.rs`
- Test: `codex-rs/core/src/custom_prompts.rs`

**Step 1: Write failing prompt discovery test for nested commands**

```rust
#[tokio::test]
async fn discovers_nested_command_names() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    std::fs::create_dir_all(root.join("prime")) .unwrap();
    std::fs::write(root.join("prime").join("vue.md"), "body").unwrap();

    let found = discover_prompts_in_tree(root).await;
    let names: Vec<String> = found.into_iter().map(|p| p.name).collect();
    assert_eq!(names, vec!["prime:vue"]);
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p codex-core custom_prompts::tests::discovers_nested_command_names`
Expected: FAIL with missing helper.

**Step 3: Implement plugin component integration**

```rust
pub async fn discover_prompts_in_tree(dir: &Path) -> Vec<CustomPrompt> {
    // 递归扫描子目录，使用 "dir:subdir:filename" 生成命名（中文注释）
}

// codex.rs list_custom_prompts:
// - load $CODEX_HOME/prompts (flat)
// - load plugin command roots (tree)
// - dedupe by name, project > user > default
```

**Step 4: Run test to verify it passes**

Run: `cargo test -p codex-core custom_prompts::tests::discovers_nested_command_names`
Expected: PASS.

**Step 5: Commit**

```bash
git add codex-rs/core/src/custom_prompts.rs codex-rs/core/src/codex.rs \
  codex-rs/core/src/skills/loader.rs codex-rs/core/src/skills/manager.rs \
  codex-rs/core/src/exec_policy.rs codex-rs/core/src/project_doc.rs \
  codex-rs/core/src/mcp/mod.rs

git commit -m "feat(core): load plugin components"
```

---

### Task 4: CLI plugin management

**Files:**
- Create: `codex-rs/cli/src/plugin_cmd.rs`
- Modify: `codex-rs/cli/src/main.rs`
- Test: `codex-rs/cli/tests/plugin_install_list.rs`

**Step 1: Write failing CLI test for list/install**

```rust
#[test]
fn plugin_list_empty_state() -> anyhow::Result<()> {
    let codex_home = tempfile::TempDir::new()?;
    let mut cmd = codex_command(codex_home.path())?;
    let output = cmd.args(["plugin", "list"]).output()?;
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout)?;
    assert!(stdout.contains("No plugins installed"));
    Ok(())
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p codex-cli plugin_install_list`
Expected: FAIL with unknown subcommand.

**Step 3: Implement `codex plugin` subcommands**

```rust
#[derive(Debug, clap::Parser)]
pub struct PluginCli {
    #[command(subcommand)]
    pub subcommand: PluginSubcommand,
}

#[derive(Debug, clap::Subcommand)]
pub enum PluginSubcommand {
    Install(InstallArgs),
    List(ListArgs),
    Enable(NameArgs),
    Disable(NameArgs),
    Policy(PolicyArgs),
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test -p codex-cli plugin_install_list`
Expected: PASS.

**Step 5: Commit**

```bash
git add codex-rs/cli/src/plugin_cmd.rs codex-rs/cli/src/main.rs codex-rs/cli/tests/

git commit -m "feat(cli): add plugin management commands"
```

---

### Task 5: TUI /plugins view

**Files:**
- Create: `codex-rs/tui/src/bottom_pane/plugins_view.rs`
- Modify: `codex-rs/tui/src/bottom_pane/mod.rs`
- Modify: `codex-rs/tui/src/chatwidget.rs`
- Modify: `codex-rs/tui/src/app_event.rs`
- Modify: `codex-rs/tui/src/app.rs`
- Modify: `codex-rs/tui/src/slash_command.rs`
- Test: `codex-rs/tui/src/bottom_pane/plugins_view.rs`

**Step 1: Write failing snapshot test for plugins view**

```rust
#[test]
fn renders_basic_popup() {
    let (tx_raw, _rx) = tokio::sync::mpsc::unbounded_channel();
    let tx = AppEventSender::new(tx_raw);
    let items = vec![PluginToggleItem {
        name: "everything-claude-code".to_string(),
        description: "commands, skills".to_string(),
        enabled: true,
        compliance_hint: "hooks detected".to_string(),
    }];

    let view = PluginsToggleView::new(items, tx);
    insta::assert_snapshot!(render_lines(&view, 60));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p codex-tui plugins_view`
Expected: FAIL with missing view.

**Step 3: Implement `/plugins` view and wiring**

```rust
// Add SlashCommand::Plugins
// Add AppEvent::OpenPluginsPopup and handlers
// Render list with [x] marker + compliance hint
```

**Step 4: Run test to verify it passes**

Run: `cargo test -p codex-tui plugins_view`
Expected: PASS.

**Step 5: Commit**

```bash
git add codex-rs/tui/src/

git commit -m "feat(tui): add /plugins management view"
```

---

### Task 6: Docs + verification

**Files:**
- Create: `docs/plugins.md`
- Modify: `docs/config.md` (add plugin registry + policy notes)
- Modify: `tasks/plugin-system-20260126/readme.md`
- Modify: `tasks/plugin-system-20260126/todo.md`

**Step 1: Write doc draft**

```markdown
# Plugins

Plugins install content bundles (commands, skills, rules, contexts, hooks, agents, MCP configs).

## Install

- `codex plugin install <path|url|github:owner/repo@ref>`
- `codex plugin list`
- `codex plugin enable|disable <name>`
- `codex plugin policy set <name> --allow-hooks/--allow-scripts`

## Compliance

- Structural validation (manifest + paths)
- Path traversal / symlink escape checks
- Hook / script scan (disabled by default)
```

**Step 2: Run formatting/tests**

Run: `just fmt`
Run: `cargo test -p codex-core`
Run: `cargo test -p codex-cli`
Run: `cargo test -p codex-tui`

**Step 3: Commit**

```bash
git add docs/ tasks/

git commit -m "docs: add plugin usage and verification notes"
```

---

## Execution Notes

- Use TDD: tests must fail before implementation.
- For E2E coverage, use CLI integration tests and TUI snapshot tests (Playwright is not applicable).
- Add detailed Chinese comments in critical logic (validation, extraction, policy gating).
- Run `just fix -p codex-core`, `just fix -p codex-cli`, `just fix -p codex-tui` before finalizing.
- If core/protocol changes occur, ask before running `cargo test --all-features`.
