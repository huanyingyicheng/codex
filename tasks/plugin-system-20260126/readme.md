# 插件系统交付说明

## 关键决策
- 采用 Claude 插件规范（.claude-plugin/plugin.json 与 marketplace.json）。
- 先实现内容型插件（B），为运行时代码插件（C）预留扩展位点。
- 合规检查包含结构校验与 hooks/脚本扫描；许可/来源校验只警告不阻断。
- hooks/scripts 默认不放行，需用户显式设置策略；安装时提示策略设置命令。

## 变更点
- Core：新增插件模块（manifest/registry/installer/validator/policy），接入 commands/skills/rules/contexts/mcp-configs 与自定义 prompt 发现。
- CLI：新增 `codex plugin`（install/list/enable/disable/policy set），支持 path/url/github/marketplace；缺少 marketplace 时提示。
- TUI：新增 `/plugins` 视图与 Slash 命令入口；新增插件列表视图快照测试。
- 文档：新增 `docs/plugins.md`，更新 `docs/config.md`，补充实现计划文档。
- 测试：新增 CLI `plugin_install_list` 用例与 TUI 插件快照。

## 验证结果
- 已完成格式化，通过 `just fmt` 验证，结果为成功。
- 已完成 lint 修复（前序执行 `just fix -p codex-core` / `just fix -p codex-cli` / `just fix -p codex-tui`），通过命令执行验证，结果为成功。
- 已完成 CLI 测试，通过 `cargo test -p codex-cli` 验证，结果为成功。
- 已完成 TUI 测试，通过 `cargo test -p codex-tui` 验证，结果为成功。
- 已完成 Core 测试，通过 `cargo test -p codex-core` 验证，结果为失败（9 项失败，主要为 rmcp_client/cli_stream/test_stdio_server 等外部依赖）。

## 最终交付
- 插件内容加载（B）已可用，CLI/TUI 管理与文档齐备；运行时代码插件（C）预留扩展位点。
- 待处理：core 相关测试因外部依赖失败，需要确认运行环境；是否需要运行 `cargo test --all-features` 需用户确认。
