# Transcript 规范化内容块

`TranscriptItem.content_blocks: Option<Vec<TranscriptContentBlock>>` 保留一条公开消息的块顺序，供正式 Transcript 消费；UI 不再需要辨认 Pi 的 thinking/toolCall 与 journal 的 reasoning/tool-call。新增 API 仍在草稿分支，未发布。旧 JSON 可省略字段；Rust struct 字面量构造方显式填 None。TekesUI pin 保持 `431093786de47b48cd566d0d9c0eafa088b10d68`。

## 输入与覆盖状态

- Text：原文字符串；不解析 Markdown、不分拆段落成聊天气泡。
- Reasoning：规范化展示用推理文字；是内容事实，不是展开/折叠状态机。
- ToolCall：引用既有、带 endpoint/session 作用域的 tool ID。`TranscriptSection::tool_for_block` 只在当前 section 查找，不跳到当前所选会话或别的 turn。
- Attachment：保留原 image/file/audio 内容与引用信息，不读取路径或执行预览。
- Unknown：保留未支持或字段损坏的公开块，不静默删掉数据或误当成正文。

index 只表示此消息数组中的位置，不是持久身份或阅读锚点。空数组为已知空内容；None 表示源只提供聚合/旧内容或没有足够信息证明块顺序，不能当成空消息。旧 text/reasoning/attachments 字段保持兼容；视图使用块模型时不能再把旧聚合字段重渲染一遍。

Native/DSH journal 的用户与 assistant 内容、Pi native conversation 的字符串/数组内容均规范化。Pi 的 message_update 携带完整当前消息，流式正文也保留块顺序，但没有 durable entry 时来源位置仍为未知；只有聚合 text/reasoning 的 journal preview 和本地 echo 保持 None，不伪造推理先后。用户同时提供相互冲突的 legacy text 与 structured content 时也保持未知顺序，不任取一份假装完成对账。

stream-only ToolCall 的公开 DTO 可能尚未进入当前 section；引用解析返回 None，视图应保留待确认覆盖状态，不能创建假的工具调用详情、成功结果或重发。已经进入 DTO 的调用继续复用其 name/arguments/phase/result/view，内容块不新增工具业务状态机。

## 与现有 Tekes App 的排版边界

消息、块和 footnote 的方向统一：机器正文、推理、工具、附件、会话提示及机器 footnote 全部靠左；用户输入、附件及用户 footnote 全部靠右。用户正文的多个块仍属于同一个圆角气泡，footnote 在气泡外尾端对齐；机器 footnote 与正文列左边缘对齐。主会话与任务/子代理/SideChat 使用相同规则。

对照 ClientKit TranscriptMetalActivity 和 Tekes 的 TranscriptMetalReasoningTests，推理默认放在独立折叠行，位于 narration/tools 之前。保留原始块顺序不等于把原始数组直接当视觉树，更不改变这一显示策略。正式视图的圆角、间距、字体、选择/焦点和滚动仍只消费 TekesUI 基础能力。尚无正式 Transcript Component，本模型与测试宿主均不是布局或原生阅读验收。

## 验证

新增模型/公开事件测试覆盖正文→推理→工具→后文、附件及未知块、损坏字段保留、scope-qualified tool ID、已知空与缺失内容区分、旧 JSON、section 内工具解析、完整 Pi stream 内容但缺少工具 DTO、纯附件用户输入和冲突 legacy 文本。第二公开 consumer 读取块覆盖状态和未解析工具引用，不依赖 demo 或 harness JSON。

三端离屏出口输出 structuredMessages / aggregateMessages / contentBlocks / unresolvedToolBlocks / unknownBlocks；消息覆盖总数必须与可见消息数相等。实际稳态三端验证要求工具引用均已解析，未解析 stream 情况由独立公共事件测试证明。此处仍不是消息气泡、推理折叠交互、文本选择、分页锚点、物理 IME 或 VoiceOver 验收。

本地验证：根工作区 124 tests、UI workspace 173 tests（含第二 consumer）、工具 18 tests、两处严格 Clippy、fmt、边界与固定输入校验通过。最后实际三端复验的 structured/aggregate 消息数：Native 5/0、Pi 7/1、DSH 10/0；三端稳态工具引用未解析数均为 0。Pi 保留前述本地命令 echo，不虚构 canonical 内容。三端问答回传、B 草稿隔离、后续提交和新 epoch 历史恢复继续通过。详见 [机器收据](rust-agent-ui-transcript-content-20261020.json)。

视觉统一消费 TekesUI 组件默认值和语义 tokens。未来显式升级固定依赖后，库的整体风格变化应直接体现，业务组件不另存一套颜色、字体或控件样式表。左右对齐与 footnote 归属按上述 Agent 排版规则保持。
