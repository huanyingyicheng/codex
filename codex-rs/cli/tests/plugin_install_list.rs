use std::path::Path;

use anyhow::Result;
use tempfile::TempDir;

fn codex_command(codex_home: &Path) -> Result<assert_cmd::Command> {
    let mut cmd = assert_cmd::Command::new(codex_utils_cargo_bin::cargo_bin("codex")?);
    cmd.env("CODEX_HOME", codex_home);
    Ok(cmd)
}

#[test]
fn plugin_list_empty_state() -> Result<()> {
    let codex_home = TempDir::new()?;

    let mut cmd = codex_command(codex_home.path())?;
    let output = cmd.args(["plugin", "list"]).output()?;
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout)?;
    assert!(stdout.contains("No plugins installed"));
    Ok(())
}

#[test]
fn plugin_install_and_list() -> Result<()> {
    let codex_home = TempDir::new()?;
    let plugin_root = TempDir::new()?;
    std::fs::write(
        plugin_root.path().join("plugin.json"),
        r#"{"name":"demo","description":"Demo plugin"}"#,
    )?;

    let mut install = codex_command(codex_home.path())?;
    install
        .args([
            "plugin",
            "install",
            plugin_root.path().to_string_lossy().as_ref(),
        ])
        .assert()
        .success();

    let mut list = codex_command(codex_home.path())?;
    let output = list.args(["plugin", "list"]).output()?;
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout)?;
    assert!(stdout.contains("demo"));
    assert!(stdout.contains("enabled"));
    Ok(())
}
