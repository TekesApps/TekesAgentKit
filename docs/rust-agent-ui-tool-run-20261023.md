# 连续工具分组：受控事实与原控件复用

只读对照 ClientKit 的 `TranscriptMetalActivity` 与 `TranscriptToolRowModel.continuousSummary`，本轮在 AgentKit 的 presentation 和 Rust 业务组件中增加连续工具分组，仍只消费 TekesUI 通用控件。

`TranscriptToolRunSummary` 接收原 canonical 顺序，不自行推断相邻关系。拒绝空 run、重复/空 tool ID 和跨回合成员；复用原 tool-row 失败/停止状态优先级，汇总运行预览、失败/停止数量以及按首次成功出现顺序归并的工具名称计数。摘要只读取允许的语义字段，截断到 96 个扩展字素；未知工具不将任意参数、凭据字段或原始 JSON 搬进常显标题。URL 预览去掉用户名、密码、查询和 fragment，中文/空格路径解码；Rust URL parser 的规范化仍需与 Foundation 的相对/特殊 URL 边界逐项核对，不声称全输入等价。

公开 `ToolRun::new(namespace, member_namespaces, scope, labels, tool_labels)` 可由第二宿主直接使用。`update` 使用原 publication/epoch 和 RunScope；默认折叠多成员组，单成员没有额外组标题。分组和成员的 disclosure 互不替代；关闭时移动并保留原控件，旧隐藏复制动作不产生意图；重新展开恢复原文本 owner。删除后重新出现的成员分配新命名空间，旧按钮不能获得新授权。

Transcript 根据已知事件顺序折叠同回合连续工具。正文、推理、未知位置和回合边界打断分组；已知空且无可见节点的 assistant 容器不打断。前插、拆分、合并通过原 canonical 成员关联保留 wrapper 和原成员控件，不靠正文或像素猜测。消息内部对连续、已解析且唯一的 tool 引用使用同一容器；重复引用与未解析引用保持原独立显示。

## API 迁移

- `TranscriptContent::row_id(Tool(id))` 是用于阅读/导航的分组 owner；`tool_activity_id(id)` 才是原 ToolActivity 控件 ID。折叠组中原成员不在活动 Host 树中。
- `TranscriptContent::tool_run(id)`、`OutputMessage::tool_run(id)` 可读取所属组及其 disclosure ID。
- `OutputMessage::content_ids()` 与 `tool_detail_sections()` 仍按原 block 顺序返回原成员标识；复制通道只读取当前展开可见成员。
- 文案/摘要由 `ToolRunLabels` 显式提供，消息与 Transcript 的 labels 可配置相同输入。格式化回调应对同一 publication 保持确定性；名称/预览保持宿主文案边界，状态事实仍来自 presentation。

本分支 API 尚未发布，正式宿主需固定包含它们的新发布版本，不能把根 manifest 的 v0.3.8 当作这些 API 已发布的证据。

验证记录见[机器记录](rust-agent-ui-tool-run-20261023.json)。原生专项是[单独冻结构建](rust-agent-ui-native-resumed-20261023.md)，后续 URL 修改及新增测试不继承该原生结果。#53 严格嵌套阅读、#54 分隔线、富内容、完整 Source 排版与 ProcessContainer 验收仍未完成；PR #10 保持草稿。
