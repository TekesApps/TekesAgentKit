# 共享工具折叠行模型

对照 ClientKit `TranscriptToolRowModel.swift`、`TranscriptToolDisplay.swift` 和
`TranscriptMetalActivity.swift::toolNode`，新增 renderer-independent 的
`TranscriptToolRowModel` / `TranscriptToolRowState` / `TranscriptToolRowWording`。
它们是既有 `TranscriptToolCall` 的只读派生，不是新的执行或 Run 状态机。

## 输入和显示责任

`TranscriptToolCall::presentation_row(wording)` 使用规范化 phase 决定 running/ok/error/stopped。
`wording` 提供可本地化的 label、stopped，以及 title_hint、detail、progress、result、settled_action。
最后一项只能来自已格式化的工具-owned report，不能把 stdout 当作完成动作。
可通过现有 `OutputMessageLabels.tool` 回调接入宿主文案或具备完整事实的 card adapter。

选择顺序与 ClientKit 折叠行一致：

- 成功时，显式工具 report 的动作标题优先。
- 停止时只显示停止摘要。
- 运行中 progress 优先；之后为 detail、与工具名/显示名不同的 title_hint、语义参数摘要。
- 成功时任意结果首行不替代动作。
- 明确工具错误使用结果首行；仅退出码/signal/超时导致的 error 保留动作摘要。
- 参数按工具族选择，queries 合并；未知对象按排序后的键回退，encoded JSON 和结构化 JSON 均可读取。
  与 ClientKit 原规则相同，最终 raw 参数回退仍可能是完整输入；不把它描述成安全摘要或截断策略。

`visible_summary()` 和 `text()` 是共享选择结果。模型可序列化，不改原 call、结果、view 或来源位置。

## 原组件与公开消费者

原 OutputMessage / ToolActivity 使用该共享模型，仍保留原 disclosure 与展开输入/结果/view/error。
默认显示名遵循 ClientKit 的下划线 humanization 和 MCP action 名规则；宿主原文案回调继续可覆盖。
基础 SDK 的状态图形附件尚未补齐，因此原文本 phase 回退保留；不复制 icon/render/focus 实现。

原独立 consumer 的受控更新验收覆盖：

1. producer title → 新结果标题保持；成功输出不覆盖标题。
2. 显式错误显示原错误摘要，规范化 error 状态不由 isError:false 取消。
3. 更新前后原 disclosure ID、text owner、选区与焦点保持。

本轮没有捏造 journal 不提供的 progress/report，也没有把 opaque meta 猜成可显示文案。
这些输入由真实公开 producer/card 契约或显式宿主回调提供；完整 typed report、change counts、rich cards
和连续工具组仍需继续对齐。不得把该模型称为完整 ToolRow/Card 交付。

公开依赖仍为 AgentKit v0.3.8 基线和 TekesUI `50ec49f3446924fdbc66b3cff22cd8cfe150fe7c`。
新增 API 属于尚未发布的分支增量，不属于历史 tag。无 Swift、Kernel、TekesUI 修改。

root workspace 221项通过；UI workspace 547项通过、1项既有 #49忽略。root/UI严格Clippy、格式与依赖边界通过。
首轮 UI 回归在旧的小写 wire 名预期处失败，修正显示名预期后全量重跑通过；原失败日志保留。
复用现有 target，没有新建 target、复制验收程序或删除缓存，释放空间0。

验证数字和日志哈希见[机器记录](rust-agent-ui-tool-row-20261022.json)。
本轮是组件、受控公开事实及同进程宿主/独立消费者证据，没有新的真实三 harness、原生窗口、IME、
VoiceOver 或跨进程验收。用户离开桌面，原生验证暂缓。#47/#48、严格目标 footer 可见性和完整交付审计仍开放。
