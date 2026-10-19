# Transcript 来源顺序

本轮在 AgentKit presentation 保留已有公开数据中的来源位置，补齐消息与工具事件排序的输入。TekesUI pin 保持 `431093786de47b48cd566d0d9c0eafa088b10d68`，不修改 Kernel、ClientKit 或 TekesUI。没有实现正式 Transcript 视图或通用滚动/文本控件。

`TranscriptItem.source_position`、`TranscriptToolCall.source_position / result_position` 使用公共 TranscriptSourcePosition：

| 数据路径 | 已知位置 | 身份与范围 |
|---|---|---|
| Native / DSH 的 journal | sequence，可选 content_index | sequence 来自已验证 journal；工具在 assistant message 中首次出现时保留块下标，之后重复 tool/call 不另造事件 |
| Pi 的 native conversation | entry_id、entry_ordinal、可选 content_index | entry ID 来自公开 snapshot；ordinal 只用于当前已加载窗口排序，分页可改变 ordinal，不能把它当成 identity 或持久化锚点 |
| echo、未提交流式预览、旧 JSON | None | 不使用本机时间、向量位置或“最后一条 +1”补序号 |
| 有界子历史附带的父调用 | None | 父调用不属于子历史事件序列，不把父位置硬塞进子视图 |

工具调用和结果分别保留实际来源；裁剪掉调用时只知道结果位置，不反推调用位置。原有 message/tool/RPC keys 和对账路径不变，排序字段不参与提交身份。新增 JSON 字段可缺省，旧记录正常反序列化并保持未定位；Rust struct 字面量构造方需显式提供新增 Option 字段，本分支 API 尚未发布。

`TranscriptSection::event_order()` 返回：

- ordered：已证明位置的 Message 容器、ToolCall、ToolResult 引用。
- unplaced：没有来源位置的原引用；不是失败或缺失消息，消费方须明确处理其覆盖状态。

已知位置不混用 journal 与 native 两套序列。负 sequence、同位置多事件或同 ordinal 不同 native entry 会失败，不按文本/时间破平局。已知事件按公开位置排序；工具同一 DTO 的结果变更不会生成第二份工具身份。session_notices 保持独立会话卡片，不被搬进任意 turn 的事件列表。

这里证明的是事件/容器顺序。content_index 保留工具在原 assistant 内容中的位置，但现有正文和推理字符串仍可能合并多个块；它们与工具之间完整的块级渲染还需要后续规范化内容模型，不能把整个 message.text 排在工具前面就声称完成。实际工具卡片怎样展开、消息气泡、footnote、阅读锚点和分页事务仍由正式 Transcript + 公开 TekesUI API 接入验证。

根测试新增 4 个场景：相同时间的 journal 容器/调用/结果顺序、重复调用合并、Pi entry 前插后 key 与顺序关系、真实 native stream 预览未定位、旧 JSON 与不一致位置拒绝。第二公开 consumer 通过 components 重导出的公共类型读取 ordered/unplaced；demo 离屏出口和真实三端脚本核对已定位事件与未定位数。

真实 Pi 问答扩展命令保留了一条本地 provisional echo，没有 canonical entry ID；本轮把它明确保留为 unplaced，不用假序号把覆盖状态变成完整。真实脚本逐条核对未定位记录必须是无 recordId/sourceSequence 的 provisional message，普通历史和工具未知位置仍使验证失败。此命令 echo 的 canonical 对账未完成，不宣称所有历史节点都有来源位置。

本地根工作区 118、UI workspace 173（含第二 consumer）、工具 18 tests 和两处严格 Clippy、fmt、边界/实际输入检查通过。实际 Kernel 0.9.1、Pi 1.3.0、DSH 0.2.1-rc.2：已定位事件分别 7/9/14；Pi 有 1 个显式未定位的命令 echo，另两端为 0。三端问答回传、B 草稿隔离、后续提交及新 epoch 历史恢复继续通过。使用本地合成 provider；这是公共数据到组件宿主的离屏验证，不是原生消息布局/IME/阅读验收。见 [机器收据](rust-agent-ui-transcript-order-20261020.json)。

后续 [规范化内容块增量](rust-agent-ui-transcript-content-20261020.md) 为有完整公开内容的消息保留内部块顺序；只有旧聚合字段的预览仍未知。这里的 event_order 仍只是事件/容器顺序，正式视图与阅读事务的状态不变。
