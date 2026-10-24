# 工具行报告、计数与独立展示修订

本轮只读对照 ClientKit `toolNode` / `summarySpans` / `renderedChangeCount`、原工具行选择规则，以及 Tekes 的 workspace diff host adapter。新增共享 `TranscriptToolRowInputs` / `TranscriptToolHeader`，供原 ToolActivity、消息内工具和组合后的 Transcript/RunPort 共同消费；没有新增 Run 状态机或修改 SEP/Kernel。

共享规则：只有成功行采用能命名调用的显式报告动作；运行/停止/失败保持原有优先级。工具自身完整增加/删除对优先，包括 `(0,0)`，否则采用宿主当前 workspace 的完整对，选择不受 phase 限制。工作区对可能描述该文件后续编辑，不被称为调用自身的结果。保持原始 signed 值，不从 stdout、未知 meta 或私有 mirror 推断报告/数量。

Header 保存原 tool/turn ID、公开 tool DTO 的序列化指纹、共享 row、完整文字、选中的 count 和 UTF-16 spans。`secondary` 覆盖非失败行的分隔符及可见摘要；增加/删除分别是 diffAdded/diffRemoved，四个空格及 Unicode 减号与 Source 一致。这些是业务文字事实，不是字形、选区或像素算法，也不赋予执行权限。spans 相对 Header.text，不能直接作为带“展开/收起”退化前缀的按钮 label 范围；该前缀与原 disclosure/glyph 适配一起继续对齐。

## 宿主接入

```rust
let adapter = ToolHeaderAdapter {
    inputs: host_tool_header_inputs, // RunScope + public tool -> explicit immutable facts
    revision: host_tool_header_revision, // monotonic display-source revision
    name: None, // optional host display-name formatter
    stopped: || "已停止".into(),
    report_wording: host_report_wording, // shared full-phrase callbacks
};
let labels = TranscriptContentLabels::default().with_tool_header(adapter);
```

同一 builder 设置独立及消息内工具；RunPort 使用已有 `with_transcript(..., labels)`，不复制其 presentation 或控制逻辑。`ToolActivity::tool_header()` / `OutputMessage::tool_headers()` 返回当前捕获的只读投影（含折叠组内成员），不重新查询外部来源。

`revision` 属于只读展示输入，按原 Scope/connection epoch 单调；不修改或伪造 RunSnapshot.revision/connection_epoch。providers 必须读取同一 UI-turn 的不可变快照，不能在格式化中发 RPC、写操作或重试。没有外部更新的固定输入可始终返回 0。宿主报告/计数变化时推进展示修订；同一 Run 快照即可刷新标题。旧修订、同一 Run publication 下修订未变却改变投影、读入过程中修订变化、过期预备计划会在树/编辑器写入前被拒绝。该修订只授权 header 变化，不授权更改原 tool DTO、详情或 section；完整公开 tool 指纹阻止未版本化工具数据借展示更新进入同一权限帧。RunPort 同样验证原 Transcript 预备计划。

`tool_header: Some(adapter)` 优先于旧 `tool`/`scoped_tool` 的标题；旧详情 formatter 和 `tool_sections` 的原文本 owner/复制通道保留。None 保持既有路径。来源尚未公开独立 report 字段的 SEP 不被虚构出一个；真正的报告与 workspace 数据须由显式 host adapter 提供。

## 当前显示范围与上游需求

原 SDK 单行按钮显示完整文字及计数；样式继续继承 SDK。semantic spans 已捕获，但没有绘制为 secondary/增加/删除语义色。当前公开 Button/TextContent/LabelOptions 没有同一标签的 per-range foreground 输入，继续在既有 [TekesUI #25](https://github.com/TekesApps/TekesUI/issues/25) 补充单标签/原按钮消费与验收，不拆成多个 label、不自画 glyph/颜色。#38/#47 的 disclosure/glyph 仍分别跟踪，`rich_content=false`；不声称工具 chrome 或完整视觉对齐。

本分支增量未发布；公开 AgentKit 仍为 v0.3.8，UI 仍为 0.0.0 / publish=false。宿主须固定包含本 API 的完整 revision；正式发布等待新不可变 pin。

验证：根 254 项通过；完整 UI 571 项通过及 1 项既有 #49 ignore。随后补源 DTO 指纹和等价布尔表达式收尾，重新通过根全套及四套相关 UI 118 项（同一 ignore），根/UI all-targets Clippy 和 demo 构建。覆盖 UTF-16 中文/ZWJ/组合字符、完整零对、部分对回退、signed 值、失败/停止优先级、重复引用、同 ID 不同 endpoint、未版本化数据拒绝、相同 Run 快照的展示刷新、旧修订/预备计划拒绝，及原 SDK Composer 的 marked baseline/焦点/详情 owner 保留。

严格 #53 仍退出 1、父跟随未暂停并在 append 后偏移到 634、详情不可见；不忽略或放宽。本轮没有原生窗口、真实三端、物理 IME/VoiceOver 或 ProcessContainer 验收；既有冻结程序收据不转移。所有构建复用既有 target、增量关闭，无新 target/验收程序副本。共享 target、当前依赖与证据保留，本阶段没有删除或报告新增释放空间。

逐文件输入、日志与测试范围见[机器记录](rust-agent-ui-tool-header-20261023.json)。
