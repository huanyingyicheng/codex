# Codex CLI 中文文档

本文档基于仓库内现有说明整理，聚焦 Codex CLI（Rust 实现）在本仓库中的结构、使用方式与开发流程。

## 1. 项目简介

Codex CLI 是 OpenAI 推出的本地编码代理（coding agent），主要特点是：

- 在终端中交互，支持阅读/修改代码与运行命令。
- 通过沙盒与审批策略控制代理的权限范围。
- 可以在交互式 TUI 中工作，也可在非交互式场景（CI/自动化）使用。
- 支持 MCP（Model Context Protocol），可作为 MCP 客户端连接其他工具，也能启动 MCP 服务器。

本仓库包含多个实现与相关组件：

- `codex-rs/`：Rust 实现（当前主线版本）。
- `codex-cli/`：TypeScript 旧版实现（已被 Rust 版本取代）。
- `sdk/typescript/`：TypeScript SDK，用于在应用内嵌 Codex CLI。
- `shell-tool-mcp/`：实验性 MCP 服务器，提供安全的 `shell` 工具。

## 2. 核心能力概览

- **交互式 TUI**：面向终端的全屏界面，适合边改边看。
- **非交互式执行**：`codex exec` 可用于脚本或 CI 中的自动化任务。
- **审批模式**：支持从“仅建议”到“全自动”不同等级的权限控制。
- **沙盒隔离**：提供只读、工作区可写、完全开放等策略。
- **MCP 支持**：既可以连接外部 MCP 工具，也能把 Codex 作为 MCP 服务器暴露。
- **可配置与可扩展**：`config.toml`、`AGENTS.md`、`.rules` 等机制支持精细化控制。
- **SDK 封装**：SDK 用 JSONL 事件与 CLI 通信，适合嵌入式场景。

## 3. 目录结构

常见目录用途如下（以仓库根目录为准）：

- `codex-rs/`：Rust Cargo workspace，包含核心逻辑、CLI、多种子工具。
- `codex-cli/`：Legacy TS CLI（旧版实现）。
- `sdk/`：官方 SDK（目前提供 TypeScript 版本）。
- `shell-tool-mcp/`：实验性 MCP 服务器。
- `docs/`：文档与设计说明。
- `scripts/`：开发或发布辅助脚本。
- `patches/`：补丁或修复资源。
- `third_party/`：第三方依赖与许可信息。

## 4. 安装与快速开始

### 4.1 安装

推荐安装方式：

```bash
npm i -g @openai/codex
```

或使用 Homebrew：

```bash
brew install --cask codex
```

也可从 GitHub Releases 下载适合平台的二进制包并重命名为 `codex`。

### 4.2 运行

交互式模式：

```bash
codex
```

带初始提示的交互模式：

```bash
codex "explain this codebase to me"
```

非交互式模式：

```bash
codex exec "summarize repository status"
```

### 4.3 登录与鉴权

- 使用 ChatGPT 计划：在 CLI 中选择 **Sign in with ChatGPT**。
- 使用 API Key：参考 https://developers.openai.com/codex/auth

## 5. 使用方式与常用命令

### 5.1 交互式模式（TUI）

TUI 适合日常开发工作流：提问、查看输出、审批更改、运行命令等均在同一界面完成。

### 5.2 非交互式模式（`codex exec`）

适合脚本或 CI：Codex 执行一次任务后自动退出，结果直接输出到 stdout。

### 5.3 MCP 支持

- `codex mcp`：管理 MCP 服务器配置。
- `codex mcp-server`：启动 Codex 作为 MCP 服务器。

### 5.4 沙盒策略（Rust CLI）

可通过 `--sandbox` 显式选择策略：

```bash
codex --sandbox read-only
codex --sandbox workspace-write
codex --sandbox danger-full-access
```

同样可在 `~/.codex/config.toml` 中设置 `sandbox_mode`。

## 6. 安全与审批模式

Codex 提供多级审批策略（通过 `--approval-mode` 或交互式初始化设置）：

- **suggest**：只读，所有写入/命令都需人工批准。
- **auto-edit**：允许应用补丁写入，命令仍需批准。
- **full-auto**：允许写入与命令执行（在沙盒中运行）。

> 完全自动模式默认网络受限，以降低风险。

## 7. 配置与本地指令

### 7.1 配置文件位置

Rust 版本使用 `config.toml`：

```
~/.codex/config.toml
```

### 7.2 MCP 与功能开关示例

```toml
[features]
shell_tool = false

[mcp_servers.shell-tool]
command = "npx"
args = ["-y", "@openai/codex-shell-tool-mcp"]
```

### 7.3 AGENTS.md（项目指令）

支持在以下位置添加 `AGENTS.md`，Codex 会按层级合并规则：

1. `~/.codex/AGENTS.md`
2. 仓库根目录 `AGENTS.md`
3. 当前工作目录的 `AGENTS.md`

相关说明：`docs/agents_md.md`

### 7.4 配置 Schema

配置 JSON Schema 位于：

```
codex-rs/core/config.schema.json
```

## 8. SDK（TypeScript）

`@openai/codex-sdk` 通过启动本地 CLI 与之通信，适合嵌入式使用：

```typescript
import { Codex } from "@openai/codex-sdk";

const codex = new Codex();
const thread = codex.startThread();
const turn = await thread.run("Diagnose the test failure and propose a fix");

console.log(turn.finalResponse);
```

支持流式事件、结构化输出和图片输入。详见 `sdk/typescript/README.md`。

## 9. 开发与构建（Rust 版本）

### 9.1 系统要求

- macOS 12+ / Ubuntu 20.04+ / Debian 10+ / Windows 11（需 WSL2）
- RAM ≥ 4GB（推荐 8GB）
- Git 2.23+（建议）

### 9.2 从源码构建

```bash
git clone https://github.com/openai/codex.git
cd codex/codex-rs

rustup component add rustfmt
rustup component add clippy
cargo install just

cargo build
cargo run --bin codex -- "explain this codebase to me"
```

修改后常用命令：

```bash
just fmt
just fix -p <crate>
```

### 9.3 日志与调试

`RUST_LOG` 控制日志级别，TUI 默认写入：

```
~/.codex/log/codex-tui.log
```

## 10. 贡献与许可证

- 贡献指南：`docs/contributing.md`
- 许可证：`LICENSE`（Apache-2.0）

## 11. 参考链接

- 官方文档：https://developers.openai.com/codex
- 安装与构建：`docs/install.md`
- 配置：`docs/config.md`
- 认证：`docs/authentication.md`
- 执行策略：`docs/execpolicy.md`
- 非交互模式：`docs/exec.md`
- 安全与沙盒：`docs/sandbox.md`

