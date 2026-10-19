# TekesUI 候选：AgentKit 独立消费复验（2026-10-18）

状态：开发候选的自动化和真实 Native SEP 离屏链路通过；原生窗口体验未验收，不能关闭 #15/#16 或作为生产发布结论。本记录是交接后的消费侧独立复跑，不沿用上游测试计数作为本轮通过证据。

## 身份和范围

- AgentKit 已提交基线：`0dff9fc7714b93e417b9b4ed4744e6b5dcaa6373`，main CI `39095040644` 成功。该远端 CI **不覆盖当前未提交 WIP**。
- 保留交接时全部 AgentKit WIP；本轮没有改动业务或 UI 实现，没有 reset/pull、重新捕获依赖或改 TekesUI。复验使用 `ui/tekesui-source.json` 的显式候选 SHA-256：`e4c6cb6f725ff9df5ec4688927dd265d64dbbdb2380572d963c5b1017dde5bbc`。
- 清单的 188 个文件逐一核对：本地冻结副本与所选 TekesUI 工作树文件均匹配。它包含 base `bc3de79` 之外的 WIP，仍是 `development-snapshot-not-release`。
- demo 二进制 SHA-256：`46a1d12b433293c24c396457c1ca90720c68a40943f624db0e5c5e7a34124ddc`。
- Native 运行时：Kernel v0.9.1，源码提交 `ae5d6fdf32290207b646b8c99b96f2a002902cb3`；复用已构建 supervisor/worker。临时 HOME、隔离会话、本地合成 provider，不使用真实模型凭证或付费模型。

## 消费边界复核

- RunController/SessionRuntime 仍拥有业务状态。presentation 仅新增 canonical snapshot revision，供受控编辑拒绝过期数据，不新增 UI 业务状态机。
- #15：demo 从现有 controller watch 收到变化后调用公共 WindowWake；操作完成先入结果队列再 wake。原控件有限动画按需开停，无业务 frame pump；通知中再次发布及关闭停止路径有回归。
- #16：消费层把稳定 TranscriptItem.id 转为保留区间；锚点恢复、UTF-16 合法性和滚动由公共 ReadingController/TextInput 完成，不自行做像素补偿。重复/删除身份不会用相同正文重新绑定。
- #17–#20：AgentText 转交原 TextFieldContent 的布局、输入、选择、组字及受控事务；native key/pointer 文件仅翻译平台元数据，没有 fork 通用编辑/选择算法。
- #21：公共动作出口转换成产生时的 owned Intent。发送 destination、取消和分页 scope 不从处理时的当前选择重新推导。AX/排队动作与 A/B 切换有实际 controller 回归。
- UI 继续 `0.0.0` / `publish=false`，与默认 headless workspace 隔离；第二 consumer 只依赖公开组件包。

## 本轮独立复跑

| 证据层 | 结果 | 边界 |
|---|---|---|
| 格式与静态 | root/UI fmt、两项 Rust 边界检查通过 | 静态边界不是原生体验 |
| 严格 Clippy | root/UI workspace all-targets 通过 | 未跳过 warnings |
| headless | 76 项通过 | 当前 WIP 的本地结果，与已提交 main CI 分开 |
| UI + 第二 consumer | 27 项通过 | 包括版本/草稿、通知、输入路由、动作与公开挂载 |
| 实际 Earlier 布局 | 连续两页均 0pt 偏差；第二页同时 600→400 宽度、1→1.5 文字缩放 | CoreText 布局；保留选区、Composer 焦点和 A→B→A 阅读状态；非原生窗口操作 |
| 通知 | 实际 presentation 流式/取消/连接变化、空闲无业务定时通知、通知中再发布通过 | 不等于 AppKit wake 的原生多窗口验收 |
| demo 构建 | 成功 | macOS native host 编译，未启动窗口 |
| GPU fixture | 成功，已查看 PNG | 当前 light/1000×760 离屏初态，无原生窗口截图 |
| 真实 Native SEP → GPU | 认证、prompt/取消/历史/去重/重连与既有生命周期集成通过；当前 demo 离屏输出成功，已查看 PNG | 合成 provider；未声称原生交互通过 |

本轮日志：`/tmp/agentkit-handoff-{ui,headless}-{clippy,tests}.log`、`/tmp/agentkit-handoff-ui-build.log`、`/tmp/agentkit-handoff-native-sep.log`。图：`/tmp/agentkit-handoff-fixture.png`、`/tmp/agentkit-handoff-native-sep.png`。这些是本机临时证据，不是已发布构建材料。

复现（AgentKit 根目录，显式使用 Python 3.11+；当前 shell 的系统 Python 3.9 不含 tomllib）：

```sh
python3 scripts/prepare-tekesui-demo.py --source /absolute/selected-tekesui-source
python3 scripts/check-ui-boundaries.py
python3 scripts/check-headless-boundaries.py
cargo fmt --all --check
cargo fmt --manifest-path ui/Cargo.toml --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo clippy --manifest-path ui/Cargo.toml --workspace --all-targets --locked -- -D warnings
cargo test --manifest-path ui/Cargo.toml --workspace --locked -- --nocapture
cargo build --manifest-path ui/Cargo.toml --locked -p tekes-agent-ui-demo
ui/target/debug/tekes-agent-ui-demo --offscreen /tmp/agentkit-candidate.png
python3 scripts/test-real-session-endpoints.py \
  --kernel-root /absolute/kernel-0.9.1-source \
  --kernel-target-dir /absolute/existing-kernel-target \
  --ui-demo "$PWD/ui/target/debug/tekes-agent-ui-demo" \
  --ui-output /tmp/agentkit-candidate-native.png
```

## 仍待验收

1. AgentKit 原生窗口中，无持续 frame pump 的实际流式/停止/连接变化、分页位置连续性和草稿切换。已提出单独原生窗口授权问题；获得授权前不启动/操作该窗口。
2. 原生多目标 wake 隔离、拖选按住时窗口失焦、物理中文 IME、VoiceOver；性能本轮不测。既有 Gallery 或合成事件证据不能替代这些验收。
3. 上游公开不可变 commit/tag；当前候选不能改写为正式发布 pin。#15–#21 继续开放，未提交、推送、打 tag 或发布本次 WIP。

先前各阶段记录保留其源码/测试身份；最新候选状态以本记录及原交接文档为准。正式原生验收应单独附授权范围、二进制/源码身份和实际结果，不覆盖历史证据。
