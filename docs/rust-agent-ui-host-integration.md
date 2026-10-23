# Rust Agent UI：宿主短接入说明

当前为显式开发接入；版本和完整未完成项以[交付状态](rust-agent-ui-delivery-status.md)为准。
AgentKit 拥有业务组件/状态；TekesUI 拥有通用控件、输入、焦点、IME、阅读和进程能力；宿主负责窗口、整体布局、导航与系统适配。
颜色、字体、圆角和语义间距随 TekesUI defaults/tokens；机器内容/footnote 靠左，用户气泡和外置 footnote 靠右。

## 状态输入与回调

| 组件 | 输入/所有权 | 宿主回调责任 |
| --- | --- | --- |
| RunPort / Composer / Indicators | 原 RunBinding 的同一 RunSnapshot、access gate、分页/capability事实；独立 scope/namespace 保留原 editor。 | 立即捕获原 intent 和原 binding。发送先 prepare/同步清空，再 dispatch；取消/未知结果沿用原控制器，不跟随最新选中会话、不自动重试。 |
| Transcript / ReadOnlyTranscript | 原 canonical 历史、同 publication/epoch、原页游标和阅读版本；有界来源只读。 | 通过原分页请求与 `publish_run_reading_update` / `publish_readonly_reading_update` 更新。已有 reading document 时不直接替换正文或以行数/像素估算锚点。 |
| Sidebar | SidebarInput、共享 query/inventory 与偏好投影；未恢复与已确认空分开。 | 原 ContentActionRecord 的稳定值进入既有 shared dispatcher/preference store，保留 scope/source/token；不轮询“最后选中菜单”或重建操作对象。 |
| SidePanel / SidePanelNavigation | 同一 SidePanelInput、有序 tabs/selected/projection_ready；Task/child/SideChat 绑定分别持有原 pointer。 | 原 parent/tab/content token 核验异步回包；关闭 scratch 等 discard 成功才移除，旧回包不重选/复活，Missing/Loading 不变成空的新 Run。 |
| SummaryView | SummaryInput / SummaryContextInput 与原 owner；宿主可补产品 facts/formatters/slots。 | 派发捕获的 SummaryIntent；不从标题猜 pointer、不重算用量或覆盖基础事实。 |
| TurnFeedback | 原反馈 target 与 FeedbackFrame；CAS/epoch/publication 是授权输入。 | 初读/重连按原 `feedback_read_epoch()` 捕获并调用 `read_feedback_at_epoch()`，采纳前验证原 frame/source/mount。失败不是空列表；明确恢复才重读，不重放写。 |

ComponentDemo 的现成读取调度例子：在原 `take_background_commands()` 通道执行
`DemoCommand::ReadInitialFeedback(request)`，将 `request.execute().await` 返回给
`complete_initial_feedback`。离屏 snapshot 可调用 `take_initial_feedback_requests()`，保留其他后台命令。
这是 demo 的宿主调度示例；可复用的读取授权和 Frame 来自 public bindings。独立产品可直接使用同一 binding API。

compact cold/loading 由宿主原初读驱动，正常 ready 不显示 Refresh。失败/不确定状态保留恢复入口；
独立 TurnFeedback 编辑器的显式读取契约与 compact 路径区分。旧文档中“cold 必须手动 Refresh”不适用于当前 compact 宿主。

## 焦点、组字与阅读

- TekesUI 的原 TextSurface 保留 first responder、marked text、selection、undo 和 opaque baseline；业务更新先按原 controlled-draft preflight，再在同一阅读事务内重检/发布。
- AgentKit 负责组字期间的发送/导航屏障及原目的地、tab、请求身份；辅助复制等走原 SDK 保焦点策略，不替换 editor、不模拟文本来完成组字。
- 宿主提供实际可用区域、namespace、窗口唤醒和系统适配。几何只在成功 render 后作为已发布证据；不自建滚动补偿或缓存像素位置。重连/分页/追加都使用原阅读版本和事务。
- ProcessContainer 接入仍待 #48；当前原 ContentHost 路径不等于同/跨进程容器验收。缺基础属性/事件向 TekesUI 提 Issue，不复制 IPC、输入、焦点或渲染实现。

只读消息 disclosure 和 aside 的单行标题也使用原 SDK LabelOptions；宿主提供
TextSurfaceHost::shape_label 服务并保留完整 source/AX 文案，AgentKit 不提供自写 shaping/省略 fallback。
工具标题的新增共享方法与未发布范围见[标题记录](rust-agent-ui-tool-title-20261022.md)。
折叠行使用共享 `TranscriptToolCall::presentation_row(TranscriptToolRowWording)`；宿主通过原
`OutputMessageLabels.tool` 提供本地化文案或公开 card 的 progress/report，完整规则和未交付范围见
[工具行接入说明](rust-agent-ui-tool-row-20261022.md)。
`presentation_card()` 提供共享卡片事实；只在需要展开时派生，未知形状保留通用详情。
[卡片与桥接说明](rust-agent-ui-tool-card-20261022.md)区分公开失败与旧退出标记；完整 selectable code/diff Content 仍待 #25。
展开内容使用共享 `presentation_details(TranscriptToolDetailWording)`；宿主显式提供 producer
progress/detail 和本地化 copy，按稳定 section kind 挂载原 SDK 控件。当前默认详情文字消费了
该模型；独立文本区和原复制通道已接入，见[控件与 API 迁移](rust-agent-ui-tool-section-controls-20261023.md)。
结构化工具的 `OutputContentIds::body` 是容器；通过 `tool_detail_sections()` 获取各区原文本 ID。
默认 `tool_sections` 回调可替换；旧 `tool` 单字段正文需显式 `tool_sections: None`。
外部报告/diff 使用原 `scoped_tool(scope, tool)` 文案回调，分区回调也接收原 scope；
不得按当前选中会话或全局 tool ID 查外部事实。报告权威性、计数优先级和未接三端来源范围见
[报告与作用域契约](rust-agent-ui-tool-report-20261023.md)。
统一工具标题优先使用 `ToolHeaderAdapter`，按原 scope 提供 report、workspace 计数及本地化 wording。
同一 Run publication 内的展示变化递增独立 display revision；不修改源 tool DTO，也不伪造业务权限版本。
独立工具、消息内工具和 RunPort 共用这一入口；旧回调仍可用。精确输入与拒绝旧帧规则见
[工具标题契约](rust-agent-ui-tool-header-20261023.md)。

独立机器消息可用 `TranscriptThinkingFrame` 和 `OutputMessage::update_with_thinking` 消费 None/Compact 策略。
展示帧必须绑定同一 RunSnapshot 的 `revision`/`connection_epoch`、原 scope/turn 和独立展示修订；
接入后不能用旧 `update` 跳过策略恢复隐藏正文。来源现由 `RunSnapshot::thinking_frame` 或 `PaneSnapshot::thinking_frame` 从同一 snapshot 生成，
包括只读 pane 自己的 publication；顺序按 dialect projector 定义，不要求宿主缓存/推算。
TranscriptContent、RunPort 和 ReadOnlyTranscript 可用 `set_thinking_display` 在原 prepare 前传入显示偏好/计时；
旧 plan 随显示修订失效，发布仍经过原 reading token。初次 Composer 仍先 update。详细顺序与剩余布局范围见
[整帧思考输入](rust-agent-ui-thinking-parent-20261023.md)及[来源契约](rust-agent-ui-thinking-source-20261023.md)。

限高详情仅通过 `OutputMessage::with_bounded_details` / `ToolActivity::with_bounded_details` 显式试验；
标准 RunPort/Transcript 保持原文档流。嵌套阅读严格检查当前失败，等待 #53；
见[限高实验说明](rust-agent-ui-bounded-details-20261022.md)，不得用宿主滚动补偿替代 SDK 修复。

## 可运行参考与验证

第二 consumer 的[公开组件挂载/整帧发布](../ui/consumer/src/lib.rs)提供
`new_agent_run_port`、`publish_original_run` 和 `publish_original_readonly`；组件包负责实际业务实现。
默认 demo 使用[原宿主绑定](../ui/demo/src/component_demo.rs)和[反馈调度](../ui/demo/src/component_feedback.rs)。

构建方式与显式 SEP 配置见 [UI README](../ui/README.md)。沿用已有 CARGO_TARGET_DIR，固定 manifest/lock/SDK 输入；
验证分别记录静态/组件、受控 endpoint、真实 harness、GPU、原生 IME/AX及跨进程，CI 不替代窗口验收。
发布前使用含所需共享增量的新不可变 AgentKit pin；开发 path 和实验 UI 不能冒充已发布契约。

来源工厂还负责同 frame 的 reasoning carrier 与规范思考正文；受控详情合成一行并修剪首尾空白，
不改原消息 DTO/整消息复制。宿主不自行比较正文选载体。精确范围及独立 kind/时间线剩余项见
[思考载体契约](rust-agent-ui-reasoning-carrier-20261023.md)。

TranscriptContent/RunPort/只读 pane 已直接消费 `TranscriptSection::display_order` 的共享容器展示顺序；
`event_order` 继续用于原始事件证据，宿主不把原 seq 改成展示 anchor，也不自行重排。
源目录必须来自当前真实 scope；只读 child 不能简单重新标记父快照。
详细契约与本轮 IME 首键异常见[展示顺序记录](rust-agent-ui-display-order-20261023.md)。

`RunSnapshot::turn_planes(turn_id)` 和原组件 `TranscriptContent::turn_planes` 提供 activity/output/narration facts，
正常宿主无需判断 live-owner。原 `TranscriptFootnoteTiming::for_run` 已复用 presentation 的
`live_presentation_turn`。Working 被动行随同一发布的 owner 生命周期产生/退休，
copy 可用 `TranscriptContentLabels.activity_working` 配置；不承担焦点或输入实现。
分区 facts 不授予动作权限；其原始 API 和冻结程序证据见
[分区事实阶段](rust-agent-ui-turn-planes-20261023.md)。

`TranscriptTurnPlanes.activity` 进一步提供 `TranscriptActivityPresentation`：Working、
Settled 的默认展开值、回合内追加输入的 BetweenInputs，以及来源不足/直接任务来源的 Direct。
这套判定由共享 presentation 产生；宿主不要根据运行字符串、正文或输入行下标重复推断。
默认展开不是受控展开选择，后续 Activity 容器仍须保留用户选择和原控件身份。
共享判定的来源与限制见[Activity 规则](rust-agent-ui-activity-policy-20261023.md)。

普通回合现已接入实际 Activity/Output 分区与历史折叠，见[最新接入与验收](rust-agent-ui-physical-planes-20261023.md)。
Message 根保留正文；reasoning/工具由 `MessageFragment` 行的原节点持有。
查询全消息 part ID 使用 `TranscriptContent::message_content_ids`，查询 Activity 控件使用对应
`OutputMessageFragment`；宿主必须整体发布 Transcript，不能单独更新已拆分的 Message 根。

`RunPortIntent::TranscriptLayoutChanged { scope }` 只通知本地布局变化。宿主转交同一捕获快照，
通过原 SDK reading transaction 发布；reading display revision 从当前 token 递增，不复用 provider revision。
`set_activity_expanded` 的本地选择及旧计划门禁由 Kit 维护，只读 pane 使用自己的历史 publication。
组字期间只放行 Kit 提供的当前挂载 `activity_action_ids()` 和原复制控件，保留其余 mutation 门禁。
新增 `TranscriptContentLabels.activity_settled` 为 copy callback；完整字面量需补字段或使用 Default update。

回合内追加输入使用 `TranscriptActivitySegmentKey { turn_id, end }`，其中
`BeforeInput(TranscriptMessageIdentity)` 是原输入身份，`Final` 是末段；不要用行下标、正文或时间生成键。
`set_activity_segment_expanded` 同时提供在 TranscriptContent、RunPort 与 ReadOnlyTranscript，
只接受本 view 当前已安装的非空可交互段。只读 pane 继续使用自己的 publication；段键不授予会话访问权限。
Previous 计数按连续工具分组后的语义条目计算，使用 `TranscriptContentLabels.activity_previous` 配置文案。
连续输入之间没有空 Previous 按钮；末段总有状态，空末段为被动标签。包括异常结束在内的分段默认折叠，
原正文最终输出独立保留。与普通 Activity 相同，本地选择使旧计划失效，随后仍整体发布同一捕获快照；
不要在宿主复制折叠状态机。当前分段验收见[输入分段](rust-agent-ui-activity-segments-20261023.md)。

System prompt 来源由共享 projector 提供，消费 `TranscriptItem::system_prompt()` 的
`TranscriptSystemPrompt { placement, anchor_sequence }`，不要从 System 正文或行位置猜角色。
原 source sequence 仍是记录位置；显示 anchor 仅用于当前投影排版，不能当 session pointer 或 durable key。
`TranscriptPlane::InitialPrompt` 独立于 Activity/Output，必须在用户输入之前保留；Update/Inline 属于时间线。
`OutputContentKind::SystemPrompt` 沿用原消息 disclosure；copy hook 新增
`OutputMessageText::{SystemPrompt,SystemPromptUpdate}`，穷尽 match 的开发消费者需同步。
无 turn 的 header prompt 属于 session notices，不创建运行回合。详见
[来源与显示规则](rust-agent-ui-system-prompts-20261023.md)。这些 API 尚未发布，宿主仍须固定完整 AgentKit revision。

开场输入消费 `TranscriptItem::input_role()` 与 `TranscriptInputRole::{Opening,Inner}`，
该事实绑定当前 item/turn 身份；旧 DTO 无事实时保持 None，不按正文、时钟或行下标补造。
Kit 负责 Initial prompt → 已证明的开场输入 → Activity 的原控件布局，
追加输入即使前面没有机器内容，也走输入间分段。角色事实不授予导航、读取或提交权限。
