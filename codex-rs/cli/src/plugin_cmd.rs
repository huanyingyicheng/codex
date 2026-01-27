use std::path::Path;
use std::path::PathBuf;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use clap::ValueEnum;
use codex_common::CliConfigOverrides;
use codex_core::config::Config;
use codex_core::plugins::InstallOutcome;
use codex_core::plugins::PluginInstaller;
use codex_core::plugins::PluginPolicy;
use codex_core::plugins::PluginRegistry;
use codex_core::plugins::PluginRegistryEntry;
use codex_core::plugins::PluginScope;
use codex_core::plugins::PluginStore;
use url::Url;

#[derive(Debug, clap::Parser)]
pub struct PluginCli {
    #[clap(flatten)]
    pub config_overrides: CliConfigOverrides,

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

#[derive(Debug, clap::Parser)]
pub struct InstallArgs {
    /// Local path, URL, github:owner/repo@ref, or marketplace:<name>.
    pub source: String,

    /// Install scope for the plugin.
    #[arg(long, value_enum, default_value_t = PluginScopeArg::User)]
    pub scope: PluginScopeArg,

    /// Optional marketplace index path for name-based installs.
    #[arg(long)]
    pub marketplace: Option<PathBuf>,
}

#[derive(Debug, clap::Parser)]
pub struct ListArgs {
    /// Filter by scope (default: list all scopes).
    #[arg(long, value_enum)]
    pub scope: Option<PluginScopeArg>,
}

#[derive(Debug, clap::Parser)]
pub struct NameArgs {
    pub name: String,

    /// Optional scope override.
    #[arg(long, value_enum)]
    pub scope: Option<PluginScopeArg>,
}

#[derive(Debug, clap::Parser)]
pub struct PolicyArgs {
    #[command(subcommand)]
    pub command: PolicyCommand,
}

#[derive(Debug, clap::Subcommand)]
pub enum PolicyCommand {
    Set(PolicySetArgs),
}

#[derive(Debug, clap::Parser)]
pub struct PolicySetArgs {
    pub name: String,

    #[arg(long)]
    pub allow_hooks: Option<bool>,

    #[arg(long)]
    pub allow_scripts: Option<bool>,

    #[arg(long, value_enum)]
    pub scope: Option<PluginScopeArg>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum PluginScopeArg {
    User,
    Project,
}

impl PluginCli {
    pub async fn run(self) -> Result<()> {
        let overrides = self
            .config_overrides
            .parse_overrides()
            .map_err(anyhow::Error::msg)?;
        let config = Config::load_with_cli_overrides(overrides)
            .await
            .context("failed to load configuration")?;

        match self.subcommand {
            PluginSubcommand::Install(args) => run_install(&config, args).await,
            PluginSubcommand::List(args) => run_list(&config, args),
            PluginSubcommand::Enable(args) => run_toggle(&config, args, true),
            PluginSubcommand::Disable(args) => run_toggle(&config, args, false),
            PluginSubcommand::Policy(PolicyArgs {
                command: PolicyCommand::Set(args),
            }) => run_policy_set(&config, args),
        }
    }
}

#[derive(Debug)]
enum ResolvedInstallSource {
    LocalPath(PathBuf),
    Url(Url),
    GitHub {
        repo: String,
        reference: Option<String>,
    },
    Marketplace {
        index_path: PathBuf,
        name: String,
    },
}

async fn run_install(config: &Config, args: InstallArgs) -> Result<()> {
    let store = store_for_scope(config, args.scope)?;
    let marketplace_path = resolve_marketplace_path(config, args.marketplace);
    let marketplace_exists = marketplace_path.as_ref().is_some_and(|path| path.exists());

    let resolved = resolve_install_source(&args.source, marketplace_path.as_deref())?;
    // 关键逻辑：marketplace 缺失时仍允许直接安装，但需要提示用户。
    if !marketplace_exists && !matches!(resolved, ResolvedInstallSource::Marketplace { .. }) {
        println!(
            "Marketplace index not configured; name-based installs are disabled. \
Use --marketplace <path> to enable."
        );
    }

    let outcome = match resolved {
        ResolvedInstallSource::LocalPath(path) => {
            let path_display = path.display();
            PluginInstaller::install_from_path(&store, &path)
                .await
                .with_context(|| format!("failed to install from {path_display}"))?
        }
        ResolvedInstallSource::Url(url) => PluginInstaller::install_from_url(&store, &url)
            .await
            .with_context(|| format!("failed to install from {url}"))?,
        ResolvedInstallSource::GitHub { repo, reference } => {
            PluginInstaller::install_from_github(&store, &repo, reference.as_deref())
                .await
                .with_context(|| format!("failed to install from github:{repo}"))?
        }
        ResolvedInstallSource::Marketplace { index_path, name } => {
            if !index_path.exists() {
                let path_display = index_path.display();
                bail!("marketplace index not found: {path_display}");
            }
            PluginInstaller::install_from_marketplace(&store, &index_path, &name)
                .await
                .with_context(|| format!("failed to install {name} from marketplace"))?
        }
    };

    print_install_summary(&outcome);
    Ok(())
}

fn run_list(config: &Config, args: ListArgs) -> Result<()> {
    let stores = stores_for_list(config, args.scope)?;
    let mut items = Vec::new();
    for store in stores {
        let registry = PluginRegistry::load(&store.registry_path())?;
        for entry in registry.plugins {
            items.push(build_list_item(&store, &entry));
        }
    }

    if items.is_empty() {
        println!("No plugins installed. Try `codex plugin install <path|url>`.");
        return Ok(());
    }

    items.sort_by(|a, b| {
        a.name
            .cmp(&b.name)
            .then_with(|| scope_rank(a.scope.clone()).cmp(&scope_rank(b.scope.clone())))
    });
    print_list_table(&items);
    Ok(())
}

fn run_toggle(config: &Config, args: NameArgs, enabled: bool) -> Result<()> {
    let store = resolve_store_for_name(config, args.scope, &args.name)?;
    let mut registry = PluginRegistry::load(&store.registry_path())?;
    let Some(entry) = registry.find_entry_mut(&args.name) else {
        bail!("plugin {} is not installed", args.name);
    };
    entry.enabled = enabled;
    registry.save(&store.registry_path())?;
    let state = if enabled { "enabled" } else { "disabled" };
    println!("Plugin '{}' {state}.", args.name);
    Ok(())
}

fn run_policy_set(config: &Config, args: PolicySetArgs) -> Result<()> {
    if args.allow_hooks.is_none() && args.allow_scripts.is_none() {
        bail!("no policy changes requested");
    }

    let store = resolve_store_for_name(config, args.scope, &args.name)?;
    let mut registry = PluginRegistry::load(&store.registry_path())?;
    let Some(entry) = registry.find_entry_mut(&args.name) else {
        bail!("plugin {} is not installed", args.name);
    };

    let mut updated = PluginPolicy {
        allow_hooks: entry.policy.allow_hooks,
        allow_scripts: entry.policy.allow_scripts,
    };
    if let Some(value) = args.allow_hooks {
        updated.allow_hooks = value;
    }
    if let Some(value) = args.allow_scripts {
        updated.allow_scripts = value;
    }
    entry.policy = updated.clone();
    registry.save(&store.registry_path())?;
    println!(
        "Updated policy for '{}' (allow_hooks={}, allow_scripts={}).",
        args.name, updated.allow_hooks, updated.allow_scripts
    );
    Ok(())
}

fn resolve_install_source(
    source: &str,
    marketplace_path: Option<&Path>,
) -> Result<ResolvedInstallSource> {
    if let Some(rest) = source
        .strip_prefix("github:")
        .or_else(|| source.strip_prefix("gh:"))
    {
        let (repo, reference) = split_github_reference(rest)?;
        return Ok(ResolvedInstallSource::GitHub { repo, reference });
    }

    if let Ok(url) = Url::parse(source)
        && matches!(url.scheme(), "http" | "https")
    {
        return Ok(ResolvedInstallSource::Url(url));
    }

    if source.starts_with("marketplace:") {
        let name = source.trim_start_matches("marketplace:").trim().to_string();
        let Some(index_path) = marketplace_path.map(PathBuf::from) else {
            bail!("marketplace index not configured");
        };
        return Ok(ResolvedInstallSource::Marketplace { index_path, name });
    }

    let local = PathBuf::from(source);
    if local.exists() {
        return Ok(ResolvedInstallSource::LocalPath(local));
    }

    if let Some(path) = marketplace_path {
        return Ok(ResolvedInstallSource::Marketplace {
            index_path: path.to_path_buf(),
            name: source.to_string(),
        });
    }

    bail!(
        "unknown plugin source: {source}. Use a path, URL, github:owner/repo@ref, or --marketplace."
    );
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

fn resolve_marketplace_path(config: &Config, override_path: Option<PathBuf>) -> Option<PathBuf> {
    override_path.or_else(|| Some(config.codex_home.join("marketplace.json")))
}

fn store_for_scope(config: &Config, scope: PluginScopeArg) -> Result<PluginStore> {
    match scope {
        PluginScopeArg::User => Ok(PluginStore::user_scope(&config.codex_home)),
        PluginScopeArg::Project => {
            let root = find_project_root(&config.cwd);
            Ok(PluginStore::project_scope(&root.join(".codex")))
        }
    }
}

fn stores_for_list(config: &Config, scope: Option<PluginScopeArg>) -> Result<Vec<PluginStore>> {
    match scope {
        Some(scope) => Ok(vec![store_for_scope(config, scope)?]),
        None => {
            let mut stores = vec![PluginStore::user_scope(&config.codex_home)];
            let root = find_project_root(&config.cwd);
            stores.push(PluginStore::project_scope(&root.join(".codex")));
            Ok(stores)
        }
    }
}

fn resolve_store_for_name(
    config: &Config,
    scope: Option<PluginScopeArg>,
    name: &str,
) -> Result<PluginStore> {
    let stores = stores_for_list(config, scope)?;
    let mut matches = Vec::new();
    for store in stores {
        let registry = PluginRegistry::load(&store.registry_path())?;
        if registry.find_entry(name).is_some() {
            matches.push(store);
        }
    }

    match matches.as_slice() {
        [store] => Ok(store.clone()),
        [] => bail!("plugin {name} is not installed"),
        _ => bail!("plugin {name} exists in multiple scopes; use --scope"),
    }
}

fn find_project_root(cwd: &Path) -> PathBuf {
    let mut cursor = cwd;
    loop {
        let git_marker = cursor.join(".git");
        if git_marker.exists() {
            return cursor.to_path_buf();
        }
        let Some(parent) = cursor.parent() else {
            return cwd.to_path_buf();
        };
        cursor = parent;
    }
}

struct PluginListItem {
    name: String,
    scope: PluginScope,
    enabled: bool,
    description: String,
    compliance: String,
}

fn build_list_item(store: &PluginStore, entry: &PluginRegistryEntry) -> PluginListItem {
    let root = store.plugin_dir(&entry.name);
    let description = match codex_core::plugins::PluginManifest::load_from(&root) {
        Ok(manifest) => manifest.description.unwrap_or_else(|| "-".to_string()),
        Err(_) => "manifest missing".to_string(),
    };
    let compliance = compliance_hint(entry);

    PluginListItem {
        name: entry.name.clone(),
        scope: entry.scope.clone(),
        enabled: entry.enabled,
        description,
        compliance,
    }
}

fn compliance_hint(entry: &PluginRegistryEntry) -> String {
    if entry.compliance.warnings.is_empty() {
        return "-".to_string();
    }
    entry.compliance.warnings.join("; ")
}

fn print_list_table(items: &[PluginListItem]) {
    let name_width = items
        .iter()
        .map(|item| item.name.len())
        .max()
        .unwrap_or(4)
        .max("Name".len());
    let scope_width = items
        .iter()
        .map(|item| scope_label(item.scope.clone()).len())
        .max()
        .unwrap_or(5)
        .max("Scope".len());

    println!(
        "{name:<name_width$} {status:<8} {scope:<scope_width$} Description Compliance",
        name = "Name",
        status = "Status",
        scope = "Scope",
        name_width = name_width,
        scope_width = scope_width,
    );
    for item in items {
        let status = if item.enabled { "enabled" } else { "disabled" };
        let scope = scope_label(item.scope.clone());
        println!(
            "{name:<name_width$} {status:<8} {scope:<scope_width$} {description} {compliance}",
            name = item.name,
            status = status,
            scope = scope,
            description = item.description,
            compliance = item.compliance,
            name_width = name_width,
            scope_width = scope_width,
        );
    }
}

fn scope_label(scope: PluginScope) -> &'static str {
    match scope {
        PluginScope::User => "user",
        PluginScope::Project => "project",
    }
}

fn scope_rank(scope: PluginScope) -> u8 {
    match scope {
        PluginScope::Project => 0,
        PluginScope::User => 1,
    }
}

fn print_install_summary(outcome: &InstallOutcome) {
    let name = &outcome.entry.name;
    let scope = scope_label(outcome.entry.scope.clone());
    let root_display = outcome.root.display();
    println!("Installed plugin '{name}' ({scope}) at {root_display}.");

    if !outcome.entry.compliance.warnings.is_empty() {
        println!("Warnings:");
        for warning in &outcome.entry.compliance.warnings {
            println!("- {warning}");
        }
    }

    if outcome.entry.compliance.hooks_detected || outcome.entry.compliance.scripts_detected {
        println!(
            "Set policy: codex plugin policy set {name} --allow-hooks true --allow-scripts true"
        );
    }
}
