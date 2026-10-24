# 工具结果状态与原始元数据

在 7e9a651 的 producer 标题派生基础上，对照当前 ClientKit `SessionEventProjector` 的 tool/result 分支与
JSON `int` 读取规则，补齐已有 journal 方言的工具结算事实。Tekes 宿主继续委托 ClientKit 的 Run 内容。
精确公开 TekesUI 仍为 `50ec49f3446924fdbc66b3cff22cd8cfe150fe7c`；未修改 SDK、Kernel 或 Swift 源码。

## 结算规则

原 fold 只读取首个 content 的 isError 和顶层 meta.exitCode，导致 signal、超时、持久 sessionStatus、
正式 result view 与后续 content 的失败显示成 done。新增回归先观察到 4 失败/1通过，保留退出101的原日志。

现在沿用现有 error/done/stopped 规范化值，无新状态机或执行动作：

1. 结构化 `error.code=interrupted` 优先为 stopped，不用 signal 字符串猜取消。
2. 结构化 error 对象、message.isError、任一 content.isError 标记失败；false/null/字符串/数组 error 不当作结构化错误。
3. meta 的非零 exitCode、字符串 signal、timedOut=true，以及 `sessionStatus.kind=exited` 的非零 exitCode/字符串 signal 标记失败。
4. 仅正式 `{for: result, view: ...}` 的退出状态参与；call/future/未标记 view 不越权，但原始值仍保留。
5. 数字按当前 ClientKit 的 JSON Double→精确64位整数语义读取，接受1.0，拒绝非整数、字符串及越界值；0/-0无失败。
   提供的空字符串 signal 与源实现一致，仍属于显式 signal 字段。数值规则的第二次修改前回归同样保留退出101。

工具失败不决定其父回合失败。实时/恢复后的 tools、原结果、ID及位置保持，已完成回合仍按独立 turn/end 事实结算。
这里不改变 Pi native conversation 的独立 isError 语义，也不猜其私有 details 字段。

## 保留公开数据与兼容性

`TranscriptToolCall` 新增 `result_meta` / `result_error`，JSON 为可选 resultMeta/resultError，
保留公开 tool/result 的非 null 原 meta/error；未知非 null 形状保持 opaque，不变成失败信号。
null 按可选字段缺失处理。`result` 仍是原 message，不混入额外字段。
这为后续 report/terminal/card模型保留原事实，避免 UI 回头读取 harness 内部文件或重算执行状态。

两个字段都有 serde default/None省略，旧快照缺字段可读取；新快照及未知形状 round-trip 已验证。
直接使用 Rust struct literal 的开发消费者需补 `result_meta: None, result_error: None`。
这是尚未发布的共享 API 增量，不属于现有 v0.3.8 tag，也没有修改该历史 tag。

内嵌/独立工具的原默认详情保留原 message/view，并在结构化结果错误存在时补原错误详情。
不把普通未知 error 字段渲染成错误；不在 UI 重新从 isError:false 推断工具成功。
独立 consumer 对 error→原详情、同一 owner/焦点/选区的保持留有回归。

## 验证和范围

- root workspace **208** 项通过，其中 presentation 包 **121** 项，含8项新增投影/数字/恢复/序列化用例。
- UI workspace **547** 项通过，1项既有 TekesUI #49 忽略；含宿主和第二公开 consumer。
- root/UI workspace/all-targets 严格 Clippy、格式与 headless/UI 边界通过。

本轮为共享投影、原组件和受控公有回包的同进程证据；没有真实 harness 故障、原生窗口、系统 IME、
VoiceOver 或跨进程验收。此前冻结程序的三端/原生结果保留原身份，不迁移为本轮结果。
完整工具 row/card 的 progress/report/计数/错误摘要和 rich views 仍需继续对齐；#25/#47/#48 等缺口不关闭，
目标 footer 的严格可见性失败仍未解决。PR #10保持draft，不合并或tag。

[机器记录](rust-agent-ui-tool-result-status-20261022.json)保存改动/对照源码与所有阶段日志哈希。
原始数据在 `.runtime-fixtures/tool-result-status-20261022/`。复用原 target，无新 target/验收程序副本；
没有删除缓存或证据，释放空间0。
