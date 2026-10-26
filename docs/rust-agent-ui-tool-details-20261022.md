# 工具展开内容：共享分区契约

`TranscriptToolCall::presentation_details(TranscriptToolDetailWording)` 返回纯显示事实，
不持有执行、分页、焦点或滚动状态。UI 组件入口重新导出分区类型，第二消费者无需私有模块。
这是当前开发分支的新 API，不属于已发布 v0.3.8。

规则对照 ClientKit 的 `detailNodes` / `ioDetailNodes` 及实际 Projector→View 桥接：

- 分区顺序固定为 Input、Output，再接实际存在的 Call view、Result view；缺失视图不生成“未提供”占位。
- Input 保留原参数；仅空白输入按 `will` 区分“正在接收输入”与“输入不可用”。
- Output 按结果、显式 producer progress、progress detail 顺序累加，过滤空白项并去掉相同内容。
  不从 opaque meta、相邻调用或折叠标题猜测进度。
- 运行中未回传结果时 Output 为空；已回传的空结果与没有结果分开。
  只有未回传且已结束的调用可用宿主提供的 display fallback。
- 失败标题使用 canonical 工具失败；旧 shell 输出末尾退出标记不会额外设置 canonical Failed。
  结构化错误即使没有消息体，也保留 `name: code` 摘要。
- 每区 kind 在同一 tool identity 下稳定。显示最多移除一个尾部 LF，`copy_text` 保留原文；
  View JSON 保留完整可解析值。原 DTO、元数据和工具/父回合结算不变。

默认 OutputMessage/ToolActivity 的详情文字已消费共享分区，直接显示结果正文，
不再把整个结果消息对象与空视图占位混在正文里。宿主仍可通过原 `labels.tool` 回调提供文案；
需要 producer progress 的宿主可显式调用同一模型。原 disclosure 与 text owner 保留。

**独立 Input/Output 文本区、各区复制动作、富卡片和最终排版尚未交付。**
当前组件仍将分区串接在原只读文本字段中；不能把本轮模型/文字消费当作独立视口验收。
限高详情仍只允许显式实验入口；标准路径保持 inline，等待 [TekesUI #53](https://github.com/TekesApps/TekesUI/issues/53)。

测试和 source/证据哈希见[机器记录](rust-agent-ui-tool-details-20261022.json)。
原生、真实 harness、系统 IME、VoiceOver、ProcessContainer 均无本轮新增通过证据。
