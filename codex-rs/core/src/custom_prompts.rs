use codex_protocol::custom_prompts::CustomPrompt;
use std::collections::HashSet;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;
use tokio::fs;
use tokio::task::spawn_blocking;
use walkdir::WalkDir;

/// Return the default prompts directory: `$CODEX_HOME/prompts`.
/// If `CODEX_HOME` cannot be resolved, returns `None`.
pub fn default_prompts_dir() -> Option<PathBuf> {
    crate::config::find_codex_home()
        .ok()
        .map(|home| home.join("prompts"))
}

/// Discover prompt files in the given directory, returning entries sorted by name.
/// Non-files are ignored. If the directory does not exist or cannot be read, returns empty.
pub async fn discover_prompts_in(dir: &Path) -> Vec<CustomPrompt> {
    discover_prompts_in_excluding(dir, &HashSet::new()).await
}

/// Recursively discover prompt files under `dir`, naming them as `dir:subdir:filename`.
pub async fn discover_prompts_in_tree(dir: &Path) -> Vec<CustomPrompt> {
    let dir = dir.to_path_buf();
    spawn_blocking(move || discover_prompts_in_tree_blocking(&dir))
        .await
        .unwrap_or_default()
}

fn discover_prompts_in_tree_blocking(dir: &Path) -> Vec<CustomPrompt> {
    let mut out: Vec<CustomPrompt> = Vec::new();
    if !dir.exists() {
        return out;
    }

    let walker = WalkDir::new(dir)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| {
            if entry.depth() == 0 {
                return true;
            }
            entry
                .file_name()
                .to_str()
                .is_some_and(|name| !name.starts_with('.'))
        });

    for entry in walker.flatten() {
        let path = entry.path();
        if entry.file_type().is_symlink() {
            continue;
        }
        if !entry.file_type().is_file() {
            continue;
        }
        let is_md = path
            .extension()
            .and_then(|s| s.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("md"));
        if !is_md {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let rel = match path.strip_prefix(dir) {
            Ok(rel) => rel,
            Err(_) => continue,
        };

        let mut parts: Vec<String> = Vec::new();
        for component in rel.components() {
            if let Component::Normal(name) = component {
                let Some(text) = name.to_str() else {
                    parts.clear();
                    break;
                };
                parts.push(text.to_string());
            }
        }
        if parts.is_empty() {
            continue;
        }
        // 关键逻辑：用相对路径组件生成命名，最后一段替换为文件名（不含扩展）。
        if let Some(last) = parts.last_mut() {
            *last = stem.to_string();
        }
        let name = parts.join(":");
        let content = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(_) => continue,
        };
        let (description, argument_hint, body) = parse_frontmatter(&content);
        out.push(CustomPrompt {
            name,
            path: path.to_path_buf(),
            content: body,
            description,
            argument_hint,
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// Discover prompt files in the given directory, excluding any with names in `exclude`.
/// Returns entries sorted by name. Non-files are ignored. Missing/unreadable dir yields empty.
pub async fn discover_prompts_in_excluding(
    dir: &Path,
    exclude: &HashSet<String>,
) -> Vec<CustomPrompt> {
    let mut out: Vec<CustomPrompt> = Vec::new();
    let mut entries = match fs::read_dir(dir).await {
        Ok(entries) => entries,
        Err(_) => return out,
    };

    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        let is_file_like = fs::metadata(&path)
            .await
            .map(|m| m.is_file())
            .unwrap_or(false);
        if !is_file_like {
            continue;
        }
        // Only include Markdown files with a .md extension.
        let is_md = path
            .extension()
            .and_then(|s| s.to_str())
            .map(|ext| ext.eq_ignore_ascii_case("md"))
            .unwrap_or(false);
        if !is_md {
            continue;
        }
        let Some(name) = path
            .file_stem()
            .and_then(|s| s.to_str())
            .map(str::to_string)
        else {
            continue;
        };
        if exclude.contains(&name) {
            continue;
        }
        let content = match fs::read_to_string(&path).await {
            Ok(s) => s,
            Err(_) => continue,
        };
        let (description, argument_hint, body) = parse_frontmatter(&content);
        out.push(CustomPrompt {
            name,
            path,
            content: body,
            description,
            argument_hint,
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// Parse optional YAML-like frontmatter at the beginning of `content`.
/// Supported keys:
/// - `description`: short description shown in the slash popup
/// - `argument-hint` or `argument_hint`: brief hint string shown after the description
///   Returns (description, argument_hint, body_without_frontmatter).
fn parse_frontmatter(content: &str) -> (Option<String>, Option<String>, String) {
    let mut segments = content.split_inclusive('\n');
    let Some(first_segment) = segments.next() else {
        return (None, None, String::new());
    };
    let first_line = first_segment.trim_end_matches(['\r', '\n']);
    if first_line.trim() != "---" {
        return (None, None, content.to_string());
    }

    let mut desc: Option<String> = None;
    let mut hint: Option<String> = None;
    let mut frontmatter_closed = false;
    let mut consumed = first_segment.len();

    for segment in segments {
        let line = segment.trim_end_matches(['\r', '\n']);
        let trimmed = line.trim();

        if trimmed == "---" {
            frontmatter_closed = true;
            consumed += segment.len();
            break;
        }

        if trimmed.is_empty() || trimmed.starts_with('#') {
            consumed += segment.len();
            continue;
        }

        if let Some((k, v)) = trimmed.split_once(':') {
            let key = k.trim().to_ascii_lowercase();
            let mut val = v.trim().to_string();
            if val.len() >= 2 {
                let bytes = val.as_bytes();
                let first = bytes[0];
                let last = bytes[bytes.len() - 1];
                if (first == b'\"' && last == b'\"') || (first == b'\'' && last == b'\'') {
                    val = val[1..val.len().saturating_sub(1)].to_string();
                }
            }
            match key.as_str() {
                "description" => desc = Some(val),
                "argument-hint" | "argument_hint" => hint = Some(val),
                _ => {}
            }
        }

        consumed += segment.len();
    }

    if !frontmatter_closed {
        // Unterminated frontmatter: treat input as-is.
        return (None, None, content.to_string());
    }

    let body = if consumed >= content.len() {
        String::new()
    } else {
        content[consumed..].to_string()
    };
    (desc, hint, body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[tokio::test]
    async fn empty_when_dir_missing() {
        let tmp = tempdir().expect("create TempDir");
        let missing = tmp.path().join("nope");
        let found = discover_prompts_in(&missing).await;
        assert!(found.is_empty());
    }

    #[tokio::test]
    async fn discovers_and_sorts_files() {
        let tmp = tempdir().expect("create TempDir");
        let dir = tmp.path();
        fs::write(dir.join("b.md"), b"b").unwrap();
        fs::write(dir.join("a.md"), b"a").unwrap();
        fs::create_dir(dir.join("subdir")).unwrap();
        let found = discover_prompts_in(dir).await;
        let names: Vec<String> = found.into_iter().map(|e| e.name).collect();
        assert_eq!(names, vec!["a", "b"]);
    }

    #[tokio::test]
    async fn excludes_builtins() {
        let tmp = tempdir().expect("create TempDir");
        let dir = tmp.path();
        fs::write(dir.join("init.md"), b"ignored").unwrap();
        fs::write(dir.join("foo.md"), b"ok").unwrap();
        let mut exclude = HashSet::new();
        exclude.insert("init".to_string());
        let found = discover_prompts_in_excluding(dir, &exclude).await;
        let names: Vec<String> = found.into_iter().map(|e| e.name).collect();
        assert_eq!(names, vec!["foo"]);
    }

    #[tokio::test]
    async fn skips_non_utf8_files() {
        let tmp = tempdir().expect("create TempDir");
        let dir = tmp.path();
        // Valid UTF-8 file
        fs::write(dir.join("good.md"), b"hello").unwrap();
        // Invalid UTF-8 content in .md file (e.g., lone 0xFF byte)
        fs::write(dir.join("bad.md"), vec![0xFF, 0xFE, b'\n']).unwrap();
        let found = discover_prompts_in(dir).await;
        let names: Vec<String> = found.into_iter().map(|e| e.name).collect();
        assert_eq!(names, vec!["good"]);
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn discovers_symlinked_md_files() {
        let tmp = tempdir().expect("create TempDir");
        let dir = tmp.path();

        // Create a real file
        fs::write(dir.join("real.md"), b"real content").unwrap();

        // Create a symlink to the real file
        std::os::unix::fs::symlink(dir.join("real.md"), dir.join("link.md")).unwrap();

        let found = discover_prompts_in(dir).await;
        let names: Vec<String> = found.into_iter().map(|e| e.name).collect();

        // Both real and link should be discovered, sorted alphabetically
        assert_eq!(names, vec!["link", "real"]);
    }

    #[tokio::test]
    async fn parses_frontmatter_and_strips_from_body() {
        let tmp = tempdir().expect("create TempDir");
        let dir = tmp.path();
        let file = dir.join("withmeta.md");
        let text = "---\nname: ignored\ndescription: \"Quick review command\"\nargument-hint: \"[file] [priority]\"\n---\nActual body with $1 and $ARGUMENTS";
        fs::write(&file, text).unwrap();

        let found = discover_prompts_in(dir).await;
        assert_eq!(found.len(), 1);
        let p = &found[0];
        assert_eq!(p.name, "withmeta");
        assert_eq!(p.description.as_deref(), Some("Quick review command"));
        assert_eq!(p.argument_hint.as_deref(), Some("[file] [priority]"));
        // Body should not include the frontmatter delimiters.
        assert_eq!(p.content, "Actual body with $1 and $ARGUMENTS");
    }

    #[tokio::test]
    async fn discovers_nested_command_names() {
        let tmp = tempdir().expect("tempdir");
        let root = tmp.path();
        std::fs::create_dir_all(root.join("prime")).unwrap();
        std::fs::write(root.join("prime").join("vue.md"), "body").unwrap();

        let found = discover_prompts_in_tree(root).await;
        let names: Vec<String> = found.into_iter().map(|p| p.name).collect();
        assert_eq!(names, vec!["prime:vue"]);
    }

    #[test]
    fn parse_frontmatter_preserves_body_newlines() {
        let content = "---\r\ndescription: \"Line endings\"\r\nargument_hint: \"[arg]\"\r\n---\r\nFirst line\r\nSecond line\r\n";
        let (desc, hint, body) = parse_frontmatter(content);
        assert_eq!(desc.as_deref(), Some("Line endings"));
        assert_eq!(hint.as_deref(), Some("[arg]"));
        assert_eq!(body, "First line\r\nSecond line\r\n");
    }
}
