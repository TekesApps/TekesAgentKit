# TekesUI #22 / #28 公开依赖与原生复验

2026-10-22 核对公开 main 与 peeled tag `remediation-toolbar-checkpoint-20261021`，均为 `09b4bab44b6dfa65487250ea816b610b9df59095`。AgentKit 的 `ui/Cargo.toml` / `ui/Cargo.lock` 已精确消费此提交，本轮无需改 pin。[#22](https://github.com/TekesApps/TekesUI/issues/22) 和 [#28](https://github.com/TekesApps/TekesUI/issues/28) 仍 Open；issue 状态不能用于推断公开 API 缺失。

现有原生 demo 已接通原 SDK 的持续 reading-document 更新、分页锚点、Sidebar 选中项自动 reveal 与回合导航。普通回合使用纵轴 Start，最后回合使用 follow_latest；独立 rail 的首次/目标变化 reveal 继续走库接口。原 token、版本、挂载与业务来源校验保持，不借焦点跳转，不反算 offset。实现与第二 consumer 的记录见[公开导航接入](rust-agent-ui-published-navigation-20261021.md)和[最新公开 SDK 接收](tekesui-toolbar-public-intake-20261021.md)。

## 本轮实际验证

复用当前分支 `cbaae6ff2b68417ae88a9ae9229044d03466211b` 的既有原生验收程序 `.runtime-fixtures/AgentKitCrossPaneCopy.app`，可执行文件 SHA256 `2705f5f75f46f945ccf5ee4debc38351439f2b2d59d8ae66d95e45446fae5375`，没有重新编译。

- `--reading-native-selftest` 正常退出：原生分页按钮及共享请求使内容20→44项；前插、向上阅读、同 publication refresh、A/B 阅读与草稿、重连恢复通过。Start/Latest 原控件导航保留原编辑器、焦点、选区及中文 marked baseline，无提交。这里使用 owned fixture、合成滚轮与预编辑，不代表真实三端的新原生验收。
- `scripts/test-system-ime.py` 正常退出：PID 定向按键经过系统简体拼音，前后输入源均为 `com.apple.inputmethod.SCIM.ITABC`。软件 IME 通过，实体键盘、候选窗与 VoiceOver 不在此次证据内。
- 重新生成实际 Cargo metadata，校验8个 SDK 包、241个已审输入，清单 SHA256 `44b3174022584d3c099a45b86892d17dbf948905476c0f038be104df12aefe46`；Rust UI 边界检查通过。该静态检查与原生证据分别记录。
- 分支 `cbaae6f` 的4项 CI 均成功；本轮没有重新运行组件全套，其496项测试结果沿用[同一程序的验收记录](rust-agent-ui-cross-pane-copy-20261022.md)。

本轮证据在 `.runtime-fixtures/public-reading-recheck-20261022/`：公开 refs、issue 状态、metadata、依赖校验、原生日志、系统 IME 收据及 CI 快照。

## 保留的失败与边界

同一程序另一次独立串行真实端验证中，Pi 原生阅读50→56通过，随后流式阅读窗口始终 inactive，探针0/9失败；后续 stream/run-controls、DSH/Native 原生阶段未执行。证据在 `.runtime-fixtures/real-native-isolated-20261022/`，不将该失败计作通过，也不归因为已经排除的终端穿插。尚未确定宿主/启动方式/SDK 的责任，不能声称已有独立基础组件复现。未加入强制激活重试或放宽焦点断言。

当前所需 #22/#28 基础 API 可消费，没有新建重复 issue；其余明确基础缺口继续由各自 TekesUI issue 跟踪。UI、完整 rail 体验和真实端原生组合仍为实验状态，PR #10 不因此合并或打稳定 tag。

本轮仅复用既有可执行文件和 target 内的 Cargo metadata；没有新建 target、编译或删除缓存，释放空间0。源码、WIP、固定依赖快照、历史失败和验收程序均保留。
