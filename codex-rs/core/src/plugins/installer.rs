use std::ffi::OsStr;
use std::fs;
use std::io::Cursor;
use std::io::Read;
use std::io::Seek;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use reqwest::Client;
use tempfile::TempDir;
use url::Url;
use walkdir::WalkDir;
use zip::ZipArchive;

use crate::plugins::manifest::PluginManifest;
use crate::plugins::policy::PluginPolicy;
use crate::plugins::registry::PluginComplianceReport;
use crate::plugins::registry::PluginRegistry;
use crate::plugins::registry::PluginRegistryEntry;
use crate::plugins::registry::PluginScope;
use crate::plugins::registry::PluginSource;
use crate::plugins::validator::PluginValidator;

const REGISTRY_FILENAME: &str = "installed_plugins.json";

#[derive(Debug, Clone)]
pub struct PluginStore {
    root: PathBuf,
    scope: PluginScope,
}

impl PluginStore {
    pub fn user_scope(codex_home: &Path) -> Self {
        Self {
            root: codex_home.join("plugins"),
            scope: PluginScope::User,
        }
    }

    pub fn project_scope(dot_codex_dir: &Path) -> Self {
        Self {
            root: dot_codex_dir.join("plugins"),
            scope: PluginScope::Project,
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn scope(&self) -> PluginScope {
        self.scope.clone()
    }

    pub fn registry_path(&self) -> PathBuf {
        self.root.join(REGISTRY_FILENAME)
    }

    pub fn plugin_dir(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }
}

#[derive(Debug, Clone)]
pub struct InstallOutcome {
    pub entry: PluginRegistryEntry,
    pub manifest: PluginManifest,
    pub root: PathBuf,
}

#[derive(Debug, Clone)]
pub struct PluginMarketplaceIndex {
    pub name: Option<String>,
    pub plugins: Vec<PluginMarketplaceEntry>,
}

#[derive(Debug, Clone)]
pub struct PluginMarketplaceEntry {
    pub name: String,
    pub source: String,
    pub description: Option<String>,
}

impl PluginMarketplaceIndex {
    pub fn load(path: &Path) -> Result<Self> {
        let path_display = path.display();
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read marketplace index {path_display}"))?;
        let raw: MarketplaceIndexRaw = serde_json::from_str(&text)
            .with_context(|| format!("invalid marketplace index {path_display}"))?;
        Ok(PluginMarketplaceIndex {
            name: raw.name,
            plugins: raw
                .plugins
                .into_iter()
                .map(|entry| PluginMarketplaceEntry {
                    name: entry.name,
                    source: entry.source,
                    description: entry.description,
                })
                .collect(),
        })
    }

    pub fn find(&self, name: &str) -> Option<&PluginMarketplaceEntry> {
        self.plugins.iter().find(|entry| entry.name == name)
    }
}

#[derive(Debug, Clone, serde::Deserialize)]
struct MarketplaceIndexRaw {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    plugins: Vec<MarketplaceEntryRaw>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct MarketplaceEntryRaw {
    name: String,
    source: String,
    #[serde(default)]
    description: Option<String>,
}

pub struct PluginInstaller;

impl PluginInstaller {
    pub async fn install_from_path(store: &PluginStore, source: &Path) -> Result<InstallOutcome> {
        let source = source.to_path_buf();
        let materialized = materialize_local_source(&source).await?;
        let entry_source = PluginSource::LocalPath(source.display().to_string());
        install_from_root(store, materialized.root(), entry_source).await
    }

    pub async fn install_from_url(store: &PluginStore, url: &Url) -> Result<InstallOutcome> {
        let bytes = download_bytes(url).await?;
        let materialized = extract_zip_bytes(bytes).await?;
        let entry_source = PluginSource::Url(url.to_string());
        install_from_root(store, materialized.root(), entry_source).await
    }

    pub async fn install_from_github(
        store: &PluginStore,
        repo: &str,
        reference: Option<&str>,
    ) -> Result<InstallOutcome> {
        let reference = reference.unwrap_or("HEAD");
        let url = format!("https://codeload.github.com/{repo}/zip/{reference}");
        let url = Url::parse(&url)?;
        let bytes = download_bytes(&url).await?;
        let materialized = extract_zip_bytes(bytes).await?;
        let entry_source = PluginSource::GitHub {
            repo: repo.to_string(),
            reference: Some(reference.to_string()),
        };
        install_from_root(store, materialized.root(), entry_source).await
    }

    pub async fn install_from_marketplace(
        store: &PluginStore,
        index_path: &Path,
        name: &str,
    ) -> Result<InstallOutcome> {
        let index = PluginMarketplaceIndex::load(index_path)?;
        let entry = index
            .find(name)
            .with_context(|| format!("plugin {name} not found in marketplace"))?;
        let resolved = resolve_marketplace_source(index_path, &entry.source)?;
        let entry_source = PluginSource::Marketplace {
            name: entry.name.clone(),
            source: entry.source.clone(),
        };
        match resolved {
            MarketplaceSource::LocalPath(path) => {
                let materialized = materialize_local_source(&path).await?;
                install_from_root(store, materialized.root(), entry_source.clone()).await
            }
            MarketplaceSource::Url(url) => {
                let bytes = download_bytes(&url).await?;
                let materialized = extract_zip_bytes(bytes).await?;
                install_from_root(store, materialized.root(), entry_source.clone()).await
            }
            MarketplaceSource::GitHub { repo, reference } => {
                let reference = reference.unwrap_or_else(|| "HEAD".to_string());
                let url = format!("https://codeload.github.com/{repo}/zip/{reference}");
                let url = Url::parse(&url)?;
                let bytes = download_bytes(&url).await?;
                let materialized = extract_zip_bytes(bytes).await?;
                install_from_root(store, materialized.root(), entry_source).await
            }
        }
    }
}

#[derive(Debug)]
struct MaterializedSource {
    _temp: Option<TempDir>,
    root: PathBuf,
}

impl MaterializedSource {
    fn root(&self) -> &Path {
        &self.root
    }
}

#[derive(Debug)]
enum MarketplaceSource {
    LocalPath(PathBuf),
    Url(Url),
    GitHub {
        repo: String,
        reference: Option<String>,
    },
}

async fn download_bytes(url: &Url) -> Result<Vec<u8>> {
    let client = Client::new();
    let response = client
        .get(url.clone())
        .send()
        .await
        .with_context(|| format!("failed to download {url}"))?
        .error_for_status()
        .with_context(|| format!("download {url} returned error status"))?;
    let bytes = response
        .bytes()
        .await
        .with_context(|| format!("failed to read body from {url}"))?;
    Ok(bytes.to_vec())
}

async fn materialize_local_source(source: &Path) -> Result<MaterializedSource> {
    if source.is_dir() {
        return Ok(MaterializedSource {
            _temp: None,
            root: source.to_path_buf(),
        });
    }

    if source.is_file() && is_zip_path(source) {
        let source_display = source.display();
        let bytes = tokio::fs::read(source)
            .await
            .with_context(|| format!("failed to read archive {source_display}"))?;
        return extract_zip_bytes(bytes).await;
    }

    let source_display = source.display();
    bail!("source is not a directory or zip archive: {source_display}");
}

fn resolve_marketplace_source(index_path: &Path, source: &str) -> Result<MarketplaceSource> {
    if let Some(source) = source.strip_prefix("github:") {
        let (repo, reference) = split_github_reference(source)?;
        return Ok(MarketplaceSource::GitHub { repo, reference });
    }

    if let Ok(url) = Url::parse(source) {
        return Ok(MarketplaceSource::Url(url));
    }

    let base = index_path.parent().unwrap_or(index_path);
    Ok(MarketplaceSource::LocalPath(base.join(source)))
}

fn split_github_reference(source: &str) -> Result<(String, Option<String>)> {
    let mut parts = source.split('@');
    let Some(repo) = parts
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        bail!("github source missing repo");
    };
    let reference = parts
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    Ok((repo.to_string(), reference.map(ToString::to_string)))
}

async fn install_from_root(
    store: &PluginStore,
    source_root: &Path,
    source: PluginSource,
) -> Result<InstallOutcome> {
    let manifest =
        PluginManifest::load_from(source_root).context("failed to load plugin manifest")?;
    let report = PluginValidator::validate(source_root, &manifest)?;
    if !report.errors.is_empty() {
        let lines = report.errors.join("\n- ");
        bail!("plugin validation failed:\n- {lines}");
    }

    let dest_root = store.plugin_dir(&manifest.name);
    if dest_root.exists() {
        let name = &manifest.name;
        bail!("plugin {name} is already installed");
    }

    let mut registry = PluginRegistry::load(&store.registry_path())?;
    if registry.find_entry(&manifest.name).is_some() {
        let name = &manifest.name;
        bail!("plugin {name} is already registered");
    }

    let store_display = store.root().display();
    fs::create_dir_all(store.root())
        .with_context(|| format!("failed to create plugin store {store_display}"))?;

    let cleanup_guard = InstallCleanup::new(&dest_root);
    copy_plugin_dir(source_root, &dest_root)?;
    cleanup_guard.disarm();

    let entry = PluginRegistryEntry {
        name: manifest.name.clone(),
        enabled: true,
        scope: store.scope(),
        source,
        policy: PluginPolicy::default(),
        compliance: PluginComplianceReport {
            errors: Vec::new(),
            warnings: report.warnings.clone(),
            hooks_detected: report.hooks_detected,
            scripts_detected: report.scripts_detected,
        },
        runtime: None,
    };

    registry.plugins.push(entry.clone());
    registry.save(&store.registry_path())?;

    Ok(InstallOutcome {
        entry,
        manifest,
        root: dest_root,
    })
}

fn is_zip_path(path: &Path) -> bool {
    path.extension()
        .and_then(OsStr::to_str)
        .is_some_and(|ext| ext.eq_ignore_ascii_case("zip"))
}

async fn extract_zip_bytes(bytes: Vec<u8>) -> Result<MaterializedSource> {
    tokio::task::spawn_blocking(move || extract_zip_bytes_blocking(bytes))
        .await
        .context("zip extraction task failed")?
}

fn extract_zip_bytes_blocking(bytes: Vec<u8>) -> Result<MaterializedSource> {
    let cursor = Cursor::new(bytes);
    let mut archive = ZipArchive::new(cursor).context("invalid zip archive")?;
    let temp = tempfile::tempdir().context("failed to create temp dir")?;
    safe_extract_zip(&mut archive, temp.path())?;
    let root = detect_plugin_root(temp.path())?;
    Ok(MaterializedSource {
        _temp: Some(temp),
        root,
    })
}

fn safe_extract_zip<R: Read + Seek>(archive: &mut ZipArchive<R>, dest: &Path) -> Result<()> {
    // 关键逻辑：逐条目校验路径与软链接，阻断目录穿越与符号链接写入。
    for index in 0..archive.len() {
        let mut file = archive.by_index(index)?;
        let name = file.name().to_string();
        let Some(rel_path) = file.enclosed_name() else {
            bail!("zip entry escapes extraction dir: {name}");
        };
        if is_zip_symlink(&file) {
            bail!("zip entry is symlink: {name}");
        }

        let out_path = dest.join(rel_path);
        if file.is_dir() {
            fs::create_dir_all(&out_path)?;
            continue;
        }

        let Some(parent) = out_path.parent() else {
            bail!("zip entry has no parent dir: {name}");
        };
        fs::create_dir_all(parent)?;
        let mut outfile = fs::File::create(&out_path)?;
        std::io::copy(&mut file, &mut outfile)?;
    }
    Ok(())
}

fn is_zip_symlink(file: &zip::read::ZipFile<'_>) -> bool {
    file.unix_mode()
        .is_some_and(|mode| (mode & 0o170000) == 0o120000)
}

fn detect_plugin_root(root: &Path) -> Result<PathBuf> {
    if PluginManifest::load_from(root).is_ok() {
        return Ok(root.to_path_buf());
    }

    let mut candidates = Vec::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            let path = entry.path();
            if PluginManifest::load_from(&path).is_ok() {
                candidates.push(path);
            }
        }
    }

    match candidates.as_slice() {
        [single] => Ok(single.to_path_buf()),
        _ => bail!("plugin manifest not found in extracted archive"),
    }
}

fn copy_plugin_dir(source: &Path, dest: &Path) -> Result<()> {
    // 关键逻辑：拷贝插件目录时拒绝软链接，避免路径逃逸与执行注入。
    let walker = WalkDir::new(source)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| entry.file_name() != OsStr::new(".git"));

    for entry in walker {
        let entry = entry?;
        let path = entry.path();
        let rel = path.strip_prefix(source).with_context(|| {
            let path_display = path.display();
            format!("failed to compute relative path for {path_display}")
        })?;
        if rel.as_os_str().is_empty() {
            continue;
        }

        if entry.file_type().is_symlink() {
            let path_display = path.display();
            bail!("symlink not allowed: {path_display}");
        }

        let target = dest.join(rel);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&target)?;
            continue;
        }

        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(path, &target)?;
    }

    Ok(())
}

struct InstallCleanup {
    path: PathBuf,
    armed: bool,
}

impl InstallCleanup {
    fn new(path: &Path) -> Self {
        Self {
            path: path.to_path_buf(),
            armed: true,
        }
    }

    fn disarm(mut self) {
        self.armed = false;
    }
}

impl Drop for InstallCleanup {
    fn drop(&mut self) {
        if self.armed {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::PluginInstaller;
    use super::PluginStore;

    #[tokio::test]
    async fn installs_local_plugin_and_updates_registry() -> anyhow::Result<()> {
        let tmp = tempfile::tempdir()?;
        let src = tmp.path().join("src");
        std::fs::create_dir_all(&src)?;
        std::fs::write(src.join("plugin.json"), r#"{"name":"demo"}"#)?;

        let store = PluginStore::user_scope(tmp.path());
        let result = PluginInstaller::install_from_path(&store, &src).await?;

        assert_eq!(result.entry.name, "demo");
        let registry = crate::plugins::registry::PluginRegistry::load(&store.registry_path())?;
        assert_eq!(registry.plugins.len(), 1);
        Ok(())
    }
}
