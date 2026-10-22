# Transcript 思考模式：共享展示策略（2026-10-23）

当前 ClientKit 区分 `none` 和 `compact`；Rust 原组件尚未提供相同的实时思考模式。此次先增加 renderer-independent 的共享策略，供后续原 Transcript 控件消费。**这不是控件接线完成或原生验收记录。**

## 对齐来源

只读核对 ClientKit 的 `Contract/TranscriptThinkingMode.swift`、`TranscriptRendering/TranscriptMetalActivity.swift`、`TranscriptThinkingModeTests.swift` 与共享设置项；Tekes 的 `SettingsRootController` 消费 `ClientGeneralSettingsConfiguration`，没有另设宿主推理状态机。精确源码哈希见[机器记录](rust-agent-ui-thinking-policy-20261023.json)。未修改 ClientKit/Tekes 的任何 WIP，未使用 Swift 编译或新增 Swift 文件。

## 公开契约

`tekes_agent_presentation` 提供 `TranscriptThinkingMode`、`TranscriptThinkingInput` 和 `TranscriptThinkingPolicy`，由 `tekes_agent_components_ui` 原样公开；第二 consumer 只调用共享策略。

- `none` 为默认模式。`compact-thinking` 旧偏好仍映射到 `compact`，存储和设置通知由宿主适配，不由 presentation 读取系统偏好。
- 输入是同一来源平面的 reasoning carrier、call 与 narration 事实，包含稳定 item ID、首次 arrival order、response frame、provisional/ended/visible-content、settlement、live owner、final output 及已投影的 elapsed seconds。
- `.none` 隐藏实时推理正文和推理 disclosure target；仅最新且未被正文/工具替换的 reasoning carrier 产生被动占位。reasoning-end 本身不移除占位；同 frame 内容可以替换它，不受不同 frame 的相对分组排序影响。
- 占位的点数由 elapsed seconds `% 3 + 1` 决定；共享策略不启动 timer，不读取时钟，不创建可展开或复制的隐藏正文目标。
- `.compact` 保留推理 disclosure 资格；只有当前 live owner 内 provisional、未结束、无正文且无同 frame call 的最新候选拥有 active 标题。
- settled 且不再是 live owner 的回合，即使偏好为 `.none`，仍保留推理 disclosure 资格。策略不拥有展开状态，不删除或重写源推理。
- 重复或空 item ID、空 frame/group ID 被拒绝。向量位置、分页窗口 ordinal、时间戳不能冒充首次 source arrival order。

这份结果只回答“哪些推理可供展开、哪个 carrier 是临时状态、哪个标题 active”，不提供 timeline 分组/排序、输入编辑、滚动、焦点、IME、进程或 session mutation authority。

## 验证

根 workspace 265 项通过，其中新策略 11 项。第二公开 UI consumer 93 项通过、1 项既有 #49 忽略；根与 UI workspace all-target strict Clippy、格式与依赖边界检查通过。UI Clippy 包含原 demo/host 与第二 consumer 的编译，但不是原生功能验收。

11 项专项覆盖偏好兼容、被动占位和点数、reasoning-end 与 settlement、较早/较晚正文、同 frame/跨 frame 工具与 narration 替换、最新 carrier 与平序 ID、compact 标题、final/lost-owner 门禁及无效身份。与 ClientKit 源测试的对应行为逐项核对，不执行 Swift 测试工具链。

## 下一步及未完成范围

1. 在 AgentKit 的原 Run/Transcript presentation 中提供来源适配，保留真实首次到达顺序与 response frame。当前 `TranscriptItem::source_position` 的 journal/native 文档位置不能直接替代 ClientKit first-arrival order；unplaced transient 更不能通过当前向量位置补造顺序。
2. 将受控偏好及逐帧策略接入现有 OutputMessage/TranscriptContent/RunPort。必须校验原 scope、publication、connection epoch 与独立展示修订；旧策略不能跟随新选择或恢复隐藏正文的旧复制/展开权限。宿主不另写这套规则。
3. 复用 SDK 原控件保留 disclosure/editor 身份，更新预备帧失效检查，并验证同一业务 publication 的设置变化、settlement、分页、重连和 retained controls。
4. 补 demo、第二 consumer 的实际控件场景，再跑真实三端与原生。Compact 内嵌阅读的完整语义仍受 TekesUI #53 严格门禁约束，不用本次纯策略测试替代。

PR #10 仍为 draft。4d31a4c 的 Linux 两项成功、macOS 两项均在严格嵌套阅读检查失败；本次保持检查和固定 SDK 不变。完整交付清单见[当前状态](rust-agent-ui-delivery-status.md)。

本轮复用既有 target，incremental 关闭，未新建 target、未复制验收程序或删除缓存。源码、测试日志、原有固定程序与其他仓库 WIP 保留。
