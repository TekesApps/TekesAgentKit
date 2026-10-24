# 工具关闭边界与迟到结果

在 64e084d 的结果事实基础上，对照当前 ClientKit `SessionEventProjector.swift` 的
assistant/message、tool/call、tool/result 和 turn/end 分支。Tekes 的
`ClientAgentHostAdapters.swift` 仍将 RunPaneController 交给 ClientRunViewController。
只修改 AgentKit 的既有 journal 投影，不修改 Swift、Kernel、TekesUI 或宿主状态机。

## 行为

此前历史工具行始终可写：turn/end 已把未完成工具设为 stopped，晚到 result 仍能将其改成 done/error；
已经完成的结果、元数据和位置也会被覆盖。关闭后无主的 result 甚至会创建新的工具行。
原先7项回归中5项失败、2项控制通过，原日志保留。

现在在一次公开历史折叠内区分活动 call 与已关闭 turn：

- turn/end 保留历史工具行，并退出该回合的活动工具集合。
- 已观察到关闭且没有活动 call 的晚到 result 被忽略，停止/完成的结果事实保持。
- 活动工具已有结果时，后来的 tool/call 不改参数、名称或 view。
- assistant 内联引用仅负责首次登记；重复引用不覆盖已登记 call，缺失/空 ID 不登记工具。
- 尚未结算的显式 call 仍能补充参数/view，保留内联首次来源位置。
- 新回合、显式新 call，以及未观察到关闭边界的 result-only 分页窗口仍接收真实结果。
  显式重新登记已退出活动集合的同 ID 时使用新来源事实，与 ClientKit 的 call-ID upsert 语义一致。

活动集合只在既有 fold 中重建，不新增公开字段、持久缓存、协议或第二套 Run 状态机。
Pi native conversation 的独立折叠保持原样。工具状态仍不决定父回合状态。

## 验证范围

9项生命周期回归覆盖实时/回放、晚到成功/失败、元数据封存、无主结果、重复声明、
首次位置、后续回合、result-only 窗口及显式重新登记。

- root workspace：217通过；UI workspace：547通过、1项既有 #49 忽略，包含原 demo 和第二公开消费者。
- root/UI all-targets 严格 Clippy、格式及 headless/UI 边界通过。
- 使用现有 `/tmp/agentkit-layout-text-292-target`，未新建 target、复制验收程序或删除缓存；释放空间0。

这是受控公开 journal 及同进程回归证据，不是新的真实 harness 故障、原生窗口、系统 IME、
VoiceOver 或跨进程验收。用户离开桌面，原生验证继续暂缓。
公开 TekesUI 仍为 `50ec49f3446924fdbc66b3cff22cd8cfe150fe7c`；#47/#48仍开放。
严格目标 footer 可见性失败、完整工具 card 与 ProcessContainer 等完整交付门槛仍未关闭。
增量尚未发布，PR #10保持 draft，不合并或打 tag。

[机器记录](rust-agent-ui-tool-lifecycle-20261022.json)保留回归统计、日志路径和哈希。
