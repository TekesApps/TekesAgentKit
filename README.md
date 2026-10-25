# TekesAgentKit

Rust Agent UI 的当前入口是[需求 → Issue → 实现 → 验证覆盖表](docs/agentkit-requirements-coverage.md)和[总 Epic #14](https://github.com/TekesApps/TekesAgentKit/issues/14)；[原交付计划](docs/rust-agent-ui-issue-plan.md)保留历史证据。实施、协议、桌面、上游消费与发布分别追踪，未合入代码保持“已验证待集成”。UI 仍为实验接入，完整验收与发布尚未完成。

[![Rust headless](https://github.com/TekesApps/TekesAgentKit/actions/workflows/ci.yml/badge.svg)](https://github.com/TekesApps/TekesAgentKit/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

**A Rust Agent client suite aligned with TekesClientKit — independent SessionEndpoints, memory projection, and renderer-free Run / Transcript / Composer state.**

与 TekesClientKit 对齐的 Rust Agent 客户端库。仓库没有 Swift；默认无 UI 构建不依赖 TekesUI、SQLite 或 harness 的内部数据库，本地连接不需要客户端磁盘 Mirror。

## 当前阶段

`v0.1.0` 保存三个 SEP 与无 UI presentation 的阶段成果；`v0.1.1` 增加公开仓库的 MIT 授权、包元数据和使用文档；`v0.2.0` 冻结任务、子代理、SideChat 与 Summary 的无 UI 状态层；`v0.3.0` 补齐 Pi 子代理与 Native 有界子历史，见 [说明](docs/child-history.md)。当前公开基线为 `v0.3.8`，已加入共享批注、线程操作与置顶关系、provider 用量和完成回合 Fork 契约，见 [Web 契约](docs/web-ui.md) 及 [变更记录](CHANGELOG.md)。API 仍处于早期阶段，crate 尚未发布到 crates.io（`publish = false`）。宿主应固定 tag/完整 commit，升级前检查变更。

| Endpoint | 验证版本 | 权威数据路径 |
|---|---|---|
| `NativeSessionEndpoint` | Kernel **0.9.1 / v4** 协议/子历史基线；**0.9.3** 已作明确 **UI-only** 验证 | bearer HTTP、WebSocket mux、journal 与 frozen-cut history |
| `PiSessionEndpoint` | Pi **1.4.0**；保留 **1.3.0** 回归基线 | 每会话原生 stdio RPC、native entry tree 活动分支 |
| `DshSessionEndpoint` | DSH **0.2.1-rc.2**，兼容 **0.2.0-rc.2** | 认证 Remote HTTP/mux、journal/page、multipart 文件 |

Kernel 0.9.3 的 UI-only 收据不包括完整生命周期与 child 协议验收；兼容范围和完整协议基线见[SEP 契约](docs/session-endpoints.md)。

Pi 1.4.0 的独立精确安装、版本对照、截断恢复及离屏证据见[兼容升级记录](docs/pi-1.4.0-compat-20261023.md)。

已实现会话地址/指针、历史恢复、文件读取/观察、附件、交互回传和显式能力检查；独立 presentation 层提供 Run、Transcript、Composer 和连接状态。Pi 保留原生 entry ID，不伪造 journal sequence。DSH 官方未提供的 ephemeral fork/discard 明确返回 Unsupported。

历史投影在内存中。Pi 的宿主指针、receipt、队列及 Goal 元数据用于会话恢复和提交去重，不是 transcript Mirror。可选远端 durable cache、正式 UI 控件、生产 launcher 和大历史性能验收尚未交付。独立 [UI 试点 workspace](ui/README.md) 固定 TekesUI 公开 Git commit，复用现有 presentation；不属于 v0.3.8 headless 发布 API，也不影响默认 headless 构建。

当前实验宿主请先看[交付状态与剩余门禁](docs/rust-agent-ui-delivery-status.md)和[短接入说明](docs/rust-agent-ui-host-integration.md)。可复用 RunPort/Sidebar/SidePanel 已有同进程可运行路径，完整布局、当前原生和 ProcessContainer 仍未验收，不能作为稳定 UI 发布。

## 开始使用

需要 Rust **1.95.0**（由 `rust-toolchain.toml` 固定）与 Python 3。普通构建/单测不需要 Kernel、Pi、DSH、Node、Swift 或模型凭证。

```sh
git clone https://github.com/TekesApps/TekesAgentKit.git
cd TekesAgentKit
cargo build --workspace --locked
cargo test --workspace --locked
```

Git 消费示例（多个包位于同一 workspace）：

```toml
[dependencies]
tekes-agent-contract = { git = "https://github.com/TekesApps/TekesAgentKit", tag = "v0.3.8" }
tekes-agent-session = { git = "https://github.com/TekesApps/TekesAgentKit", tag = "v0.3.8" }
tekes-agent-presentation = { git = "https://github.com/TekesApps/TekesAgentKit", tag = "v0.3.8" }
tekes-agent-endpoint-kernel = { git = "https://github.com/TekesApps/TekesAgentKit", tag = "v0.3.8" }
```

本项目的已推送版本 tag 不移动；对审计要求严格的构建可改用 tag 对应的完整 `rev` 并提交 Cargo.lock。开发期 `path` override 应显式配置，不依赖隐式 sibling checkout。

连接流程是：宿主启动/配置 harness → 创建 connector → 启动 `SessionRuntime` 并等待 ready → 创建/打开 session → 选配 `RunController`。完整示例和配置见 [快速入门](docs/getting-started.md)。

## 包与边界

| Crate | 职责 |
|---|---|
| `tekes-agent-contract` | wire DTO、SessionEndpoint、能力、HostServices 和文件/历史契约 |
| `tekes-agent-session` | registry、SessionRuntime、内存投影、重连、分页及公开会话 API |
| `tekes-agent-endpoint-kernel` | 独立 Native / Kernel v4 adapter |
| `tekes-agent-endpoint-pi` | 独立 Pi adapter，含基于官方 extension API 的小型 JS companion |
| `tekes-agent-endpoint-dsh` | 独立 DSH Remote adapter |
| `tekes-agent-presentation` | 无渲染器的 Run / Transcript / Composer / 连接状态及观察 API |
| `tekes-agent-headless` | 从 stdin JSON 配置的批处理 consumer，适合协议验收 |
| `tekes-agent-components` | 未来控件入口，当前仅骨架，未接 TekesUI |

宿主拥有凭证、安装、路径授权、模型/权限配置、窗口及产品能力；通过配置或 `HostServices` / `SubmissionPreparation` 显式注入。库不读取用户全局凭证，也不自动安装或切换其他产品的 runtime。

## 验证

与 GitHub Actions 相同的检查：

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
python3 scripts/check-headless-boundaries.py
```

Rust 回归覆盖协议、投影和状态层。真实运行时验收使用临时 HOME、独立状态目录和本地合成 provider，不调用付费模型；任选已经安装好的 runtime：

```sh
python3 scripts/test-real-session-endpoints.py \
  --kernel-root /absolute/kernel-0.9.0-checkout \
  --pi-root /absolute/pi-1.3.0-installation \
  --dsh-root /absolute/dsh-0.2.1-rc.2-installation
```

Kernel root 需已有兼容的 supervisor/worker/helper/workspace-service；Pi/DSH root 需包含脚本约定的 Node/CLI 布局。此验证不能由没有这些运行时的公共 CI 替代。另有 `scripts/test-real-kernel.py` 验证取消结算和冻结分页。

DSH 可安装到专用新目录，安装器记录精确包版本、integrity 与 lockfile 哈希：

```sh
python3 scripts/install-dsh-runtime.py \
  --node-root /absolute/node-24-runtime \
  --destination /absolute/new-dsh-0.2.1-rc.2-runtime
```

## 文档

- [消费侧一键验收与正式 pin 比对工具](docs/tekesui-acceptance-tools.md)
- [TekesUI 短接入契约](docs/tekesui-integration-contract.md) · [消费侧缺口](docs/tekesui-gaps.md) · [Rust UI demo](ui/README.md)
- [快速入门与 Rust API](docs/getting-started.md)
- [Headless JSON 配置与命令](docs/headless-cli.md)
- [三端能力、session pointer 与验收](docs/session-endpoints.md)
- [Run / Transcript / Composer / 连接状态](docs/headless-presentation.md)
- [Pi 混合文件/图片的真实队列与历史恢复](docs/rust-agent-ui-pi-mixed-file-results-20261025.md)
- [Native / DSH 混合附件与重连回读](docs/rust-agent-ui-native-dsh-mixed-results-20261025.md)
- [实验 UI：Pi 实时预览与落盘阅读身份](docs/rust-agent-ui-stream-identity-20261020.md)
- [实验 UI：三端流式原生窗口验收](docs/rust-agent-ui-stream-native-20261020.md)
- [实验 UI：三端取消、错误与显式恢复](docs/rust-agent-ui-run-controls-20261020.md)
- [实验 UI：Sidebar 共享查询控制器与公开搜索 RPC](docs/rust-agent-ui-sidebar-query-controller-20261021.md)
- [实验 UI：Primary Folder 分组、筛选、持久折叠及格式4边界](docs/rust-agent-ui-sidebar-folders-20261021.md)
- [实验 UI：选中导航、临时展开与手动折叠优先级](docs/rust-agent-ui-sidebar-navigation-20261021.md)
- [Kernel v4 同步、历史及恢复](docs/headless-kernel.md)
- [路线与交付边界](docs/roadmap.md)
- [阶段变更](CHANGELOG.md) · [贡献说明](CONTRIBUTING.md)
- [SEP 源码/运行时基准](docs/headless-source-baseline.json) · [presentation 参考基准](docs/presentation-source-baseline.json)

[初始评估](docs/assessment.md) 是历史记录，其中旧 Kernel v3/workspace 和当时私有仓库的描述不代表当前实现。

## License

[MIT](LICENSE)，Copyright © 2026 TekesApps。Kernel、Pi、DSH、ClientKit 参考实现和第三方依赖分别保留各自的许可证；本仓库的 MIT License 不替代它们的授权。

Tasks、Subagents、SideChat 与 Summary 的无 UI 状态、宿主输入及能力边界见 [headless-agent-panels](docs/headless-agent-panels.md)。 隔离验收可显式开启 [Summary 来源见证 feature](docs/rust-agent-ui-summary-source-witness-contract-20261024.md)，将原发布帧与同次内存输入绑定；默认构建不采样，也不把该入口作为 UI API。

Sidebar bounded paging and visibility contract: [2026-10-21 report](docs/rust-agent-ui-sidebar-paging-20261021.md).

Canonical Sidebar selected-row reads: [2026-10-21 contract](docs/rust-agent-ui-sidebar-selected-read-20261021.md).

Public TekesUI reading/reveal recheck and filter/sort paging: [2026-10-21 report](docs/rust-agent-ui-public-reading-recheck-20261021.md).

Full Sidebar folder catalog input: [2026-10-21 contract](docs/rust-agent-ui-sidebar-catalog-20261021.md).

Current experimental public UI pin and explicit native Sidebar reveal: [published checkpoint integration](docs/rust-agent-ui-published-reading-20261021.md). This supersedes the earlier public-source-absence finding; continuous reading/page-anchor acceptance remains pending.

RunPort now publishes Transcript and Composer inside original SDK reading transactions: [continuous reading contract](docs/rust-agent-ui-continuous-reading-20261021.md). CoreText and owned-fixture native page anchors are checked separately from remaining cross-harness reading acceptance.

Retained A/B reading, native pages and reconnect across all three harnesses: [reading restoration and stale-footnote fix](docs/rust-agent-ui-retained-reading-20261021.md). Fresh-host persistent checkpoints remain tracked in TekesUI #42.

The reusable Rust composition is now **RunPort** (formerly Rust RunView). Sidebar, RunPort and SidePanel target the TekesUI ProcessContainer host contract; current same-process Content remains runnable while remote input/IME/scroll/AX delivery is tracked in [TekesUI #48](https://github.com/TekesApps/TekesUI/issues/48). See the [API migration and separate execution-mode matrix](docs/rust-agent-ui-run-port-20261022.md).

Real Native/Pi/DSH Sidebar classification, archive recovery and canonical history retention: [source-bound offscreen results](docs/rust-agent-ui-real-sidebar-classification-results-20261024.md). Unknown classification stays unknown; this does not imply desktop acceptance.

Real multi-endpoint SideChat (Pi remains second): [original dispatcher, runtime cancellation and recovery](docs/rust-agent-ui-real-multi-sidechat-results-20261024.md). Desktop and ProcessContainer acceptance remain separate.

运行中队列的协议边界、DSH 通用投影与 Native 未通过项见 [busy queue 结果](docs/rust-agent-ui-native-dsh-busy-queue-results-20261025.md)。UI 继续保持试验状态。

三端客户端连接关闭、故障预算及原 Reconnect 的实际验证与 Pi 流关闭修复见 [Sidebar transport recovery](docs/rust-agent-ui-sidebar-transport-recovery-results-20261025.md)。仅无窗口组件/协议证据，UI继续保持试验状态。

当前功能基线与分层验收见 [1026 格矩阵](docs/rust-agent-ui-acceptance-matrix-20261025.md)。矩阵区分当前源码、各次历史程序与真实协议/UI证据；[Composer 最后入队前 owner 复核](docs/rust-agent-ui-submission-last-admission-20261025.md)保留原提交目的地和 RPC，不自动重发。
