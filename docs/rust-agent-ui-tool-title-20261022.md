# Transcript 工具行：统一 producer 标题派生

继续精确消费公开 TekesUI `50ec49f3446924fdbc66b3cff22cd8cfe150fe7c`。
对照当前 ClientKit `SessionEventProjector.toolRowTitle`、`SessionToolEventView` 与完整 `TranscriptToolRowModel`，
本增量先补前者的只读标题选择，不把标题方法等同于完整 row/card renderer。
Tekes 的 `ClientAgentHostAdapters` 仍把 Run 内容委托给 ClientKit；未修改 Swift、SDK 或 Kernel。

## 标题与原始数据

共享 presentation 新增 `TranscriptToolCall::presentation_title()`，不增加缓存、事件或状态机，不改序列化字段。
原 `HistoryEntry.view` / call_view / result_view 的 `{for: call|result, view: ...}` opaque envelope 保持原样；
只消费与 call/result 位置相符的正式 producer envelope，未来/错位/未标记形状保持未知，不猜字段来源。

- 非 terminal：result view 的字符串 title → call view 的字符串 title → 工具名。
- terminal 由 call view 的 `card=terminal` 确认：call description → 参数 description → call title 首行
  → 参数 command 首行 → 工具名。参数对象与编码 JSON 字符串都按现有源形状处理，坏 JSON 不猜 partial command。
- 去掉空候选并规范空白；命令首行按源 newline 规则取首个非空分段，空白首行不会跳到后文任意命令。
  空 name 与只有空白的 name 分别使用源 Preparing/Tool fallback。未知 payload、任意参数字段和 stdout 不当作标题。

默认 `OutputMessageLabels.tool` 复用该方法；保留原工具名和显式 phase，存在不同 producer 描述时增加该描述。
内嵌 OutputMessage 的工具部分与独立 ToolActivity 共用原回调。宿主仍可覆盖既有文案 formatter。
原完整参数、结果、call/result view 继续位于原折叠详情；没有从标题反推 call ID、位置、成功或权限。

工具/推理等原 disclosure 的标题声明消费 SDK `LabelOptions::single_line(TailEllipsis)`，保留完整 source/AX name；
正文、按钮 owner、展开状态、选区与焦点保留。宿主需提供原 TextSurfaceHost::shape_label 服务，
不在 AgentKit 中加省略算法、图标、渲染或输入替代实现。

## 验证

- presentation 包 **113** 项通过，其中 7 项新增标题用例覆盖优先级、terminal/非 terminal、JSON/坏形状、
  Unicode newline/空白、无 producer 时的 fallback，以及原始工具事实不被改写。
- UI workspace **547** 项通过，1 项既有 #49 忽略；包含新增内嵌工具/第二公开 consumer。
  验证真实原 disclosure 的标题、展开原始详情、更新后的同一按钮/文本 owner、焦点/选区；
  600→180→600 宽度与 1.0/1.4 字号下单行高度保持，完整 AX 文案不被截掉。
- root/UI workspace/all-targets 严格 Clippy、格式与 headless/UI 边界检查通过。

本轮为共享逻辑、原组件/受控宿主及第二公开 consumer，没有原生窗口、真实 harness、系统 IME、
VoiceOver 或跨进程验收。早期 consumer 编译用了 selection 字段而非 SDK 的 selection() 方法，
原错误日志保留，修正后完整门禁通过。

## 继续开放

完整 `TranscriptToolRowModel` 的 progress、report/change totals、错误摘要/卡片状态，以及 search/web/terminal/diff/read
富卡片仍需继续对齐；本方法不替代它们。富内容/selection #25、原 glyph #47、ProcessContainer #48 等保持原缺口，
此前严格目标 footer 可见性失败没有关闭。UI 仍为实验开发路径，PR #10 draft，不合并/tag。

这是发布 v0.3.8 之后的共享 API 增量，尚未发布；不得宣称 v0.3.8 已包含该方法。修改文件、只读对照源码与日志哈希见
[机器记录](rust-agent-ui-tool-title-20261022.json)及 `.runtime-fixtures/tool-title-20261022/`。
继续复用已有 target；没有新增 target/验收程序副本，无缓存删除，释放空间 0。
