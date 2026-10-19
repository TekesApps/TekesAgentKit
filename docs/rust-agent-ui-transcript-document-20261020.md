# Transcript 只读消息分层

`TranscriptDocument::read(&TranscriptSnapshot)` 是正式 Transcript 前的 renderer-independent 文档投影；不是 Transcript Component，也不是 RunView。它借用当前 canonical snapshot，没有第二份 Run/分页状态机，不请求 endpoint、不解析 Markdown、不计算像素或滚动补偿。最新公开 AgentKit 仍为 v0.3.8；新增 API 尚在 PR #10。TekesUI 固定版本保持不变。

## 公开输入与输出

由原 RunBinding / ChildRun 提供已有作用域的 snapshot；宿主须同时保留 RunScope、publication 和 epoch，文档本身不验证输入 authority。`TranscriptDocument` / `TranscriptSection` / `TranscriptInput` / `TranscriptOutput` / `TranscriptDelivery` / `TranscriptMessageKey` 从 presentation 公开，并经 components 包重导出供第二 consumer 使用。

- section 保留已有 turn 顺序，input、output、工具分层；被裁剪历史中的 orphan 消息/工具保留为无 turn 的 section，不造 settlement 或用量。
- input 引用整条原消息，包括所有正文块与附件；消费组件将其组合为一个靠右圆角气泡。附件为空/正文为空都不自行删除消息。
- input footnote 独立为 Pending 或 Recorded { time }。provisional 消息即使携带时间也显示 Pending；没有时间就保留 None，不使用本机当前时间。
- output 引用同一条原消息的 kind/text/reasoning/reasoning_ended/final/provisional 等事实；展开和折叠仍属于未来视图本地状态。
- 工具引用完整原 call/result/view/phase DTO。当前 DTO 没有足以统一重建工具与消息顺序的时序字段，本投影明确保留独立工具 lane，不凭工具完成状态或列表位置伪造交错顺序。
- output footnote 返回已有 turn：共享 action definitions、live/reported usage 与边界不重新计算；reportedUsage 缺失不显示成已知零。
- has_more_before、is_stale、diagnostics 原样传入。分页 loading/error/retry 及公开阅读事务仍待正式组件绑定，不把三个事实字段称作完整分页 UI。

key 在同一 RunScope 内有效。User 有 reconciliation_id 时使用 Submission key，optimistic echo 变成服务端记录、所属 turn 改变也保留这个 key；其余使用既有 Item key。不按文本匹配、不以行号为 key。分页前插与流式修改正文不会改变既有 key。缺少 reconciliation_id 时不能承诺恢复另一条记录的 identity；这不是新的对账算法。空身份、重复 item/turn/tool 身份或重复可见 submission key 会失败，避免把阅读目标绑定到错误内容。

## 行为参考与最小样式扩展

只读对照 ClientKit `TranscriptTurnFootnote.swift`、`TranscriptMetalAppBridges.swift::inputFootnoteInformation` 和 MetalTextKit `MetalVirtualLayout.swift::appendInputSurface`；Tekes 宿主 `TranscriptFootnoteWordingTests.swift` 确认 footnote 由共享包提供，运行用量与时间有独立格式规则。没有修改 Swift 或引入 Swift toolchain。

Rust 视图仍须遵守接入契约：input 靠右且自然宽度，output 靠左正文列，footnote 在内容外侧；具体圆角、间距、字体和背景消费 TekesUI tokens，不为逐像素复刻覆盖基础控件。RunSurface 的单一文本 History 不消费本模型冒充正式 Transcript。

## 验证范围

新增模型测试覆盖整条含代码/列表/附件的输入、纯附件、Pending→Recorded、RPC key 跨确认/换 turn、活动/正文/工具/footnote 原引用、前插与流式结算、裁剪 orphan 与 notice、重复/空 key 失败。第二公开 consumer 直接消费 API，不依赖 demo。

ComponentDemo 离屏出口额外将当前真实 snapshot 投影成 document，逐项核对可见消息数和工具数，并输出 documentProjection；这只证明共享数据进入文档，不证明实际消息布局、文本选择、原生阅读位置或分页锚点。TekesUI #22/#23 的正式消费门槛仍保留，后续接入不会在 AgentKit 实现替代通用滚动/文本控件。

本地验证：presentation 58 tests（新增 5 个）、UI workspace 172 tests（含第二 consumer）、工具 18 tests、两处 strict Clippy、fmt、边界与 Cargo 实际解析的 188 个 TekesUI 输入校验通过。三端真实 harness 的 documentProjection 回执见 [机器收据](rust-agent-ui-transcript-document-20261020.json)。这些检查没有证明消息气泡像素、真实输入法或阅读锚点。
