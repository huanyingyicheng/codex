use std::path::Component;
use std::path::Path;

use walkdir::WalkDir;

use crate::plugins::manifest::PluginComponent;
use crate::plugins::manifest::PluginManifest;
use crate::plugins::registry::PluginComplianceReport;

pub struct PluginValidator;

impl PluginValidator {
    pub fn validate(
        root: &Path,
        manifest: &PluginManifest,
    ) -> anyhow::Result<PluginComplianceReport> {
        // 关键校验：路径必须在插件根目录内，且不允许使用绝对路径或 .. 逃逸。
        let mut report = PluginComplianceReport::empty();
        let canonical_root = dunce::canonicalize(root)?;
        if !canonical_root.is_dir() {
            report
                .errors
                .push("plugin root is not a directory".to_string());
            return Ok(report);
        }

        // 关键校验：插件名称必须是简单标识，不能包含路径分隔符。
        if manifest.name.trim().is_empty()
            || manifest.name.contains('/')
            || manifest.name.contains('\\')
        {
            report.errors.push("plugin name is invalid".to_string());
        }

        for component in [
            PluginComponent::Commands,
            PluginComponent::Skills,
            PluginComponent::Rules,
            PluginComponent::Contexts,
            PluginComponent::Hooks,
            PluginComponent::Agents,
            PluginComponent::McpConfigs,
        ] {
            let Some(path) = manifest.component_path(component) else {
                let fallback = canonical_root.join(component.default_dir_name());
                if fallback.exists() {
                    let canonical_candidate = dunce::canonicalize(&fallback)?;
                    if !canonical_candidate.starts_with(&canonical_root) {
                        let display = fallback.display();
                        report
                            .errors
                            .push(format!("path escapes plugin root: {display}"));
                    }
                }
                continue;
            };

            if path.is_absolute() {
                report
                    .errors
                    .push(format!("path is absolute: {}", path.display()));
                continue;
            }
            if path
                .components()
                .any(|component| matches!(component, Component::ParentDir))
            {
                report
                    .errors
                    .push(format!("path escapes plugin root: {}", path.display()));
                continue;
            }

            let candidate = canonical_root.join(path);
            if !candidate.exists() {
                report
                    .errors
                    .push(format!("component path missing: {}", path.display()));
                continue;
            }

            let canonical_candidate = dunce::canonicalize(&candidate)?;
            if !canonical_candidate.starts_with(&canonical_root) {
                report
                    .errors
                    .push(format!("path escapes plugin root: {}", path.display()));
            }
        }

        // 关键校验：软链接必须指向插件根目录内部。
        for entry in WalkDir::new(&canonical_root).follow_links(false) {
            let entry = entry?;
            if entry.file_type().is_symlink() {
                report
                    .errors
                    .push(format!("symlink not allowed: {}", entry.path().display()));
            }
        }

        let hooks_path = manifest
            .resolve_component_dir(&canonical_root, PluginComponent::Hooks)
            .or_else(|| {
                let fallback = canonical_root.join("hooks");
                fallback.exists().then_some(fallback)
            });
        let mut needs_policy_warning = false;
        if let Some(path) = hooks_path
            && path.exists()
        {
            report.hooks_detected = true;
            needs_policy_warning = true;
        }

        // 关键扫描：发现脚本目录需要提醒用户设置策略。
        let mut scripts_detected = false;
        for entry in WalkDir::new(&canonical_root).follow_links(false) {
            let entry = entry?;
            if entry.file_type().is_dir() && entry.file_name() == "scripts" {
                scripts_detected = true;
                break;
            }
        }
        if scripts_detected {
            report.scripts_detected = true;
            needs_policy_warning = true;
        }

        if needs_policy_warning {
            report
                .warnings
                .push("hooks/scripts detected; policy required".to_string());
        } else if manifest.license.is_none() {
            report
                .warnings
                .push("license missing; verify compliance".to_string());
        }

        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use tempfile::tempdir;

    use super::PluginValidator;
    use crate::plugins::manifest::PluginManifest;

    #[test]
    fn rejects_traversal_paths() -> anyhow::Result<()> {
        let tmp = tempdir()?;
        let root = tmp.path().join("demo");
        std::fs::create_dir_all(&root)?;
        std::fs::write(
            root.join("plugin.json"),
            r#"{"name":"demo","commands":"../evil"}"#,
        )?;

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
        std::fs::write(
            root.join("plugin.json"),
            r#"{"name":"demo","hooks":"./hooks"}"#,
        )?;

        let manifest = PluginManifest::load_from(&root)?;
        let report = PluginValidator::validate(&root, &manifest)?;

        assert_eq!(report.warnings.len(), 1);
        assert!(report.hooks_detected);
        assert!(report.scripts_detected);
        Ok(())
    }
}
