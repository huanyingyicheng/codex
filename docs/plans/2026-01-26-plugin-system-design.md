# 插件系统设计（B 版内容插件）

## 目标
实现可安装的内容型插件，覆盖 commands、skills、rules、contexts、hooks、agents、mcp-configs。安装时执行合规校验并提示策略设置，支持本地与远程来源（1+3），可选 marketplace 索引（2）。为运行时代码插件（C）预留扩展位点，但不实现。

## 范围与非目标
- 范围：插件清单解析、合规校验、安装与注册表管理；运行时加载插件内容；CLI/TUI 管理。
- 非目标：自动更新、签名校验、运行时代码插件、远程仓库扫描与评级。

## 架构与组件
- core：`plugins` 模块负责 manifest 解析、校验、安装、registry 与 policy。校验失败阻断安装；风险项仅警告。
- cli/tui：仅展示与管理（安装/列表/启用/禁用/策略），不直接读取插件内容。
- 运行时加载：在现有命令、技能、规则、上下文、MCP 配置加载逻辑上追加插件 roots，保持“项目 > 用户 > 默认”覆盖顺序。
- C 扩展位点：定义 `PluginRuntime` trait 与 registry 的可选 runtime 字段，保留动态插件接入通道。

## 插件格式与来源
- manifest 优先读取 `.claude-plugin/plugin.json`，否则读取根 `plugin.json`。
- 支持字段：`commands/skills/rules/contexts/hooks/agents/mcp-configs`，并兼容 `mcpServers` 别名。
- 来源：
  - 1）本地目录直接安装。
  - 3）远程 git/URL/zip 下载后解包安装。
  - 2）marketplace.json 作为索引，解析出 `source` 后走同一流程；缺失索引不阻断安装，但需提示用户。

## 安装流程与合规校验
- 流程：获取源 → 读取 manifest → 校验 → 复制到插件存储 → 写入 registry。
- 校验：
  - 禁止绝对路径与 `..` 目录穿越。
  - 校验路径存在、canonicalize 后仍在插件根内。
  - 禁止 symlink。
  - hooks/scripts 扫描：若发现则生成风险提示，默认禁用，要求用户设置策略。
  - 许可信息缺失仅警告，不阻断安装。

## 注册表与策略
- registry 记录：name、enabled、scope、source、policy、compliance report。
- policy：`allow_hooks` / `allow_scripts` 默认 false。
- 禁用仅影响加载，不删除文件；`--purge` 可选删除。

## 运行时加载策略
- commands：支持嵌套目录，命名采用 `dir:subdir:filename`；避免与现有命令冲突时优先级按“项目 > 用户 > 默认”。
- skills/rules/contexts/hooks/agents：追加插件 root 到现有扫描路径，保持排序稳定。
- mcp-configs：支持 `mcp-configs` 与 `mcpServers` 字段；默认 disabled，需用户启用。

## 错误处理
- 结构错误（manifest 缺失、路径越界、symlink、无效 JSON）直接失败并输出原因。
- 合规风险（hooks/scripts、许可缺失、来源未知）输出警告并要求设置策略。

## 测试策略
- core：manifest 解析、路径校验、合规报告、registry 读写、安装复制。
- cli：`plugin install/list/enable/disable/policy` 输出与状态变更。
- tui：/plugins 视图快照测试。
- mcp：配置解析测试，确保字段缺失不崩溃。