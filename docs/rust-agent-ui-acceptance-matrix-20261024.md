# Rust Agent UI 验收矩阵（2026-10-24）

本矩阵把 Issue [#27](https://github.com/TekesApps/TekesAgentKit/issues/27) 的 UI 验收范围拆为可核对的维度。它不是“整体通过”声明：每格由 JSON 中唯一匹配的规则解析，当前基线是 AgentKit `58521a1bc581ef14c3c63c9a7bc698bbd01d9ab3` 与锁定的 TekesUI `50ec49f3446924fdbc66b3cff22cd8cfe150fe7c`。

## 范围与读法

矩阵包含 15 项需求、Native/Pi/DSH 三个 harness、in-process/cross-process 两种拓扑，以及 8 个验证层，共 **720 格**。需求覆盖 Composer 组字、会话草稿、发送/取消、流式输出、阅读时追加、分页锚点、重连去重、Views 布局、Sidebar 操作、Task、Subagent、SideChat、Summary、真实系统 IME 和 AX/VoiceOver。规则采用通配维度和更具体的覆盖项，避免把相同状态复制 720 次；检查器会展开全格，拒绝未知维度、冲突规则、遗漏格子和不完整的必需需求集合。

状态只说明对应的维度组合：`passed` 有当前且范围匹配的直接证据；`failed` 是实际执行失败；`blocked` 表示依赖的公开能力尚不可用；`unsupported` 表示该拓扑/能力不适用或 harness 明确不提供；`not_run` 表示未执行或未证明。历史成功独立列于 `historicalObservations`，不会覆盖当前状态，也不会自动成为当前 HEAD 的验收。

`observedGates` 保存本快照两个已观测门禁及其格子范围、状态和证据引用：strict #53 阅读追加门禁当前为 `failed`，DSH SideChat 能力当前为 `unsupported`。检查器要求这些格子与门禁记录及其证据一致，但不固定状态值；后续有新基线证据时，可以同步更新门禁状态、证据引用和对应格子。只改格子状态而不更换/更新门禁证据会被拒绝。

| 当前状态 | 格数 | 说明 |
| --- | ---: | --- |
| passed | 36 | 当前 CI 的共享组件测试覆盖；同一套结果映射到三端行，不是 Native/Pi/DSH 分别验收。 |
| failed | 3 | 三个 harness 的 in-process strict CI `reading_append` 格；同一严格 #53 门禁在 `58521a1` 失败。 |
| blocked | 270 | 包括正式跨进程/ProcessContainer 能力；固定 TekesUI pin 下仍受 #48 阻塞。 |
| unsupported | 136 | 不适用验证层，以及 DSH 已明确不支持的 SideChat 离屏能力。 |
| not_run | 275 | 包含全部 in-process public-consumer 行为格；本轮没有将 standalone 构造收据冒充为行为验收，也没有启动桌面、系统 IME、VoiceOver 或 harness。 |

`component_tests` 中的 36 个 passed 格共享同一套 workspace 测试收据，代表共享 fixture 的组件测试覆盖，不是三个 harness 各自运行。`public_consumer` 行为格均为 `not_run`：CI `58521a1` 的 workspace 测试不等于本轮独立 standalone consumer 步骤。

本轮另有 [#15 API/consumer audit](rust-agent-ui-api-contract-audit-20261024.md#clean-public-consumer-fixture) 与其实际 run receipt（`.runtime-fixtures/issue-batch-20261024/public-consumer-run-receipt.json`）：外部 package 独立运行成功，构造 RunPort、Sidebar、SidePanel 并挂载为 `ContentHost`。这只是构造/挂载支持证据，不满足 public-consumer 行为格，也不证明 IME、draft、send/cancel、stream、渲染、ProcessContainer 或完整 Views 行为。

`ContentHost` 只证明进程内组件/内容宿主路径；它不能满足 `ProcessContainer` 或 cross-process 格。直接合成 Preedit/commit 或软件注入文本不能证明 macOS 系统输入法路由。系统 IME 通过条件是原生窗口收据能证明 macOS 输入法路由、marked text 和 commit；物理键盘是否参与是单独记录的字段，不作为通过前提。候选窗证据可额外记录。

## 证据边界

JSON 的 `evidenceCatalog` 为每项证据记录来源 revision、报告/运行链接、需求和验证层范围，以及适用时的程序路径、SHA-256、拓扑和宿主合同。当前 CI 使用不可变 run/job URL 和明确 head SHA；历史本地收据保留各自报告 SHA、程序 SHA 与 source commit。默认检查器会对仓库内跟踪的报告重新核对文件 SHA，并从原报告复核程序身份和 source commit；它不要求忽略目录中的程序或日志存在。`--verify-local` 会额外核对本地程序、原始 artifact 和源码 revision/hash。

| 历史收据 | 可支持的历史观察 | 不能推出 |
| --- | --- | --- |
| `history-real-sidechat` | 旧 ContentHost 程序中 Native/Pi SideChat 生命周期；DSH profile 明确 capability unavailable。 | 当前版本通过、窗口渲染、ProcessContainer 或 DSH SideChat 可用。 |
| `history-activity-segments` | 旧程序中 Native/Pi/DSH 的部分窗口流式/草稿/重连场景；阅读追加和分页仍是局部证据。 | 当前 58521a1 通过；严格 #53 失败仍有效。 |
| `history-native-bounded`、`history-readonly-source-authority` | 旧程序中有限分页/阅读与离屏内容宿主观察。 | 完整 Transcript、跨进程或当前源码验收。 |
| `history-turn-output-copy` | 旧程序接收软件定向输入事件的部分行为。 | macOS 输入法路由、marked text、候选窗或 VoiceOver。 |

外部 CI 的 UI workspace 测试在独立 strict #53 检查之前通过；严格门禁随后复现父容器跟随追加，内层详情离开可视范围。这个快照将同一共享门禁映射到三个 harness 行，`observedGates` 明确标出它们共用一份 CI 收据，并非三端分别运行。当前 UI owner、TekesUI 公开 API 和 #48 阻塞仍以对应仓库/Issue 的最新状态为准。

## 检查命令

CI 默认结构与已提交报告身份核对：

```sh
PYTHONDONTWRITEBYTECODE=1 python3 scripts/check-agent-ui-acceptance-matrix.py
```

单测（本轮实现使用随 Codex 提供的 Python runtime；常规 CI 可用其 Python 3）：

```sh
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest scripts.tests.test_agent_ui_acceptance_matrix -v
```

只有在本机保留冻结 fixture 并且需要核对本地程序、原始日志/收据与 Git 源码身份时才运行：

```sh
PYTHONDONTWRITEBYTECODE=1 python3 scripts/check-agent-ui-acceptance-matrix.py --verify-local
```

检查器成功只表示矩阵完整、规则唯一且证据声明一致；它不把 `blocked`、`failed`、`unsupported` 或 `not_run` 当作需求完成。

## 本轮另行补跑的非桌面程序

主会话另用 `8335231` 的冻结程序完成三端常规协议/离屏、Native真实有界子历史和Native/Pi SideChat专项。独立consumer构造也实际通过。详见[非桌面补验](rust-agent-ui-non-desktop-results-20261024.md)及[机器收据](rust-agent-ui-non-desktop-results-20261024.json)。
这些记录保留各自程序/SHA和验证范围，与本矩阵`58521a1`的CI基线并列；没有把它们改写成历史程序身份或完整需求格的成功。尤其阅读锚点、系统IME、窗口、ProcessContainer仍未证明。
