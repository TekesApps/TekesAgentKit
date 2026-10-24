# 工具报告、变更计数与作用域文案

本轮增加共享 `TranscriptToolReport`、`TranscriptToolReportWording` 和
`TranscriptToolChangeCount`。它们只处理明确传入的事实，不新增执行状态机、Git 查询或 harness 私有读取。
规则对照 ClientKit `TranscriptToolVocabulary`、`TranscriptToolRowModel`、
`Call.renderedChangeCount` 及 Tekes 宿主的 `actions.changeCounts`。

报告使用开放的 verb/object 字符串，包含可选 subject、resultCount/resultObject、note、addedLines/removedLines。
`names_the_call()` 仅在非空 subject、完整的计数/对象或完整行增删对存在时成立；
裸 verb、只有 note 或半个增删对不能覆盖这次调用的动作摘要。完整的 `(0, 0)` 也有权威性。
`row_text(wording)` 用宿主提供的整句、数量及连接文案格式化；不硬编码语言词序，未知 vocabulary 原样进入回调。
格式化结果进入原 `presentation_row` 的 `settled_action`：仍只有成功结束的行采用报告。
运行、显式失败、停止和旧终端退出失败继续使用原共享优先级，stdout 不变成成功动作标题。

`TranscriptToolChangeCount::select(report, workspace)` 先选报告的完整增删对，再用宿主的当前工作区 diff。
不相加、不补半对、不重算 Git、不丢零值或改写有符号源值。该选择与成功/失败状态独立，符合源 View；
工作区值可能已包含同一路径后续编辑，不能冒充工具本次改动。

## 接入与来源隔离

- `OutputMessageLabels::tool` 继续支持只依赖当前 tool DTO 的纯文案。
- 新增 `scoped_tool: Option<fn(&RunScope, &TranscriptToolCall) -> (String, String)>`，有值时优先于旧 `tool`。
  外部报告、工作区值等查找必须使用这个原 scope 加 tool/turn 身份，不能按当前选中会话或全局 tool ID 查找。
- `tool_sections` 回调现在是 `fn(&RunScope, &TranscriptToolCall, OutputMessageLabels) -> TranscriptToolDetails`。
  已解析消息内工具及独立 ToolActivity 都传入组件创建时的原 scope；非法 scope 在回调前拒绝。
- 宿主应在同一个受控 publication 内提供稳定的报告、diff 和本地化事实。
  外部数据/文案变化时发布新的 UI publication，沿用既有 epoch/preflight 门禁；不在测量或绘制时重新查当前目标。

当前三个 SEP 的公共投影没有这份独立报告，ClientKit 当前 SessionEndpoint 路径也没有把旧 mirror report 注入它。
因此没有给 `TranscriptToolCall` 添加虚构 report，也没有从 `result_meta.didDo`、stdout、view 标题或私有数据库猜测。
默认三端路径保持既有无报告行为；真实宿主已有报告时可通过上述作用域回调消费共享模型。
第二消费者演示报告/计数文字接入，但默认 UI 没有新增计数视图或语义着色；它不证明真实 harness 报告闭环。

报告及 JSON 规则、成功/失败优先级、工作区 fallback 有根测试。
独立消费者检查同 ID 不同 endpoint 的隔离、报告与 opaque meta 的区分、报告结算时原文本所有者保持；
消息内工具另检查两个 endpoint 的同 session/tool ID 向原 scoped 文案和分区回调传参。
根工作区 246 项、UI 工作区 561 项通过，另有 1 项已有忽略；类型别名 lint 修正后又通过 19 项回调检查。
两套 Clippy、格式与 demo 构建通过；严格嵌套阅读仍退出 1。无本轮新增原生/真实 harness 验收。
源码及验证哈希见[机器记录](rust-agent-ui-tool-report-20261023.json)。

连续工具分组、最终计数/语义 runs、富卡片、当前程序原生对照仍未完成。
TekesUI #25/#47/#48/#53/#54 等门禁保持，严格嵌套阅读检查不放宽。
这些是未发布共享增量；正式宿主应等待新不可变 AgentKit pin，不能把它当成未修改的 v0.3.8。
