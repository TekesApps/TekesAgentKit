# Rust Agent UI：宿主短接入说明

当前为显式开发接入；版本和完整未完成项以[交付状态](rust-agent-ui-delivery-status.md)为准。
原生验收须遵循[固定宿主与外部截图入口](rust-agent-ui-native-toolchain.md)，历史程序仅归档。
任务、子代理和 SideChat 的来源/宿主边界见[SidePanel 契约核对](rust-agent-ui-sidepanel-contract-audit-20261023.md)。
AgentKit 拥有业务组件/状态；TekesUI 拥有通用控件、输入、焦点、IME、阅读和进程能力；宿主负责窗口、整体布局、导航与系统适配。
颜色、字体、圆角和语义间距随 TekesUI defaults/tokens；机器内容/footnote 靠左，用户气泡和外置 footnote 靠右。

## 当前公开接入基线（Issue #15）

本审计基于 AgentKit 源码 HEAD `58521a1bc581ef14c3c63c9a7bc698bbd01d9ab3`，TekesUI 固定公开 revision
`50ec49f3446924fdbc66b3cff22cd8cfe150fe7c`。UI workspace (`ui/`) 的包仍为 `0.0.0` 且
`publish = false`；已发布的 AgentKit `v0.3.8` 不包含当前 UI 组件及其所需的共享 presentation 增量。
因此当前只能用明确的 AgentKit 仓库开发路径和 `ui/Cargo.lock` 做验证输入，不能把它描述成可供外部用户安装的
已发布 AgentKit UI 契约。正式宿主接入需等包含所需 API 的不可变 AgentKit 发布版本；不得把活动 branch、path override
或 TekesUI 的公开 pin 当作 AgentKit UI 发布版本。

稳定的组件命名是 `RunPort`（Composer + Transcript + Indicators 组合），不是旧的 `RunView` 名称，也不是
`RunSurface`。`RunSurface` 和 `ComponentDemo` 是协议/控件测试宿主，不是复用组件 API。三个业务组件均由
`tekes-agent-components-ui` 导出；可用公开 `RunPort::standard(namespace, rows, scope)`、
`Sidebar::new(namespace, labels)`、`SidePanel::new(namespace, labels, retained_parent_limit)` 构造，
并分别用 `into_host()` 安装到 TekesUI `ContentHost`。独立 consumer 的构造示例、显式 path 和验证命令见
[consumer README](../ui/consumer/README.md)；机器可读的当前核对结果见[契约审计](rust-agent-ui-api-contract-audit-20261024.md)。

AgentKit 组件持有领域展示规则、publication 验证、原 pointer/source 权限和类型化意图；宿主持有运行时订阅、窗口与
导航、输入参数、工具/路径/安装/凭据/系统权限 adapter，以及把捕获意图送回原 dispatcher/controller 的职责。
TekesUI `ContentHost` 提供当前同进程挂载路径。固定公开 TekesUI revision 不包含可验收的 `ProcessContainer`
同进程/跨进程适配契约；当前三个容器尚未完成该集成，不能以 `into_host()` 或组件离屏测试代替。
缺少的基础能力继续跟踪 TekesUI #48/#64，等公开精确版本具备所需能力后消费。

公开业务输入/回调的字段责任如下；这些组件仍是当前未发布 UI workspace 的实验 API：

| 组件 | 宿主发布的状态 | Kit 发出的业务事件 | 宿主处理 |
| --- | --- | --- | --- |
| `RunPort` | 固定 `RunScope` 与独占 ID namespace/row pool；每帧传入原 `RunSnapshot`、`ComposerAffordances`、`ComposerControlGate`、匹配的 `TranscriptPagingFacts` 和 actionable 可用事实。 | `RunPortIntent`（Composer send/cancel/edit、load earlier、actionable、footnote/copy、reading/layout/navigation）；`prepare_update` 保留同一帧来源。 | 立即 drain 原 SDK action；按 intent 捕获的 scope/来源和原 binding dispatch。不得从当前选中会话重定向旧事件，也不得自动重试未知发送。 |
| `Sidebar` | 单调 `SidebarInput.revision`、inventory loaded/error、canonical rows、selected address；排序/分组结果、偏好和搜索来源由既有 store/query 提供。 | Select/Action、分页请求、偏好/搜索、visibility/reveal intent。 | 将动作交回现有导航、共享 action dispatcher、偏好 store 或搜索查询，复核原 address/definition/token 后再执行；Kit 不另存 inventory、pin 或偏好。 |
| `SidePanel` | 单调 `SidePanelInput.revision`、parent、`projection_ready`、ordered tabs/selected、同一 controller 的 `SideChatSnapshot` 与 `supports_side_chat`。 | Select/Close/CreateSideChat/Launch；每个内容安装携带原 `PanelContentToken`。 | 从原 task/child/chat binding 安装内容，并在异步返回时重新验证 parent/tab/token。关闭临时 chat 要等原 discard 成功；旧回包不能重选、复活内容或变成新 Run。 |

这些输入分别由 `RunPort::update` / `prepare_update`、`Sidebar::update` 与 preference/search API、
`SidePanel::update` 承载；三者都公开 `into_host()` 生成现有 `ContentHost`。`ContentAction` 必须先交给原
组件的 `take_intents` / 专用 page/preference 接口，不能改为轮询当前菜单状态。完整 public constructor
consumer 和精确开发路径见 [独立 consumer](../ui/consumer/README.md)。

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

## 派生来源与只读 API 字段（#12/#13）

`ChildPaneBinding::for_run` / `TaskPaneBinding::for_run` 保留原 Task/child/SideChat 权限，
包含不同 scope 与异步 history 返回后的门禁。Missing 不复活，Closing/Unknown 不变成可写；
主 Run 的 mount 卸载不等于取消远端执行或丢弃共享草稿。

当前分支的未发布 UI API 中，`PaneSnapshot::ReadOnlyChild` 带 `source` 实例身份；
`ReadOnlyTranscriptInput::source` 由原 binding 的 `readonly_input` 提供。
同 scope/epoch/publication 不代表同一绑定，不能将旧 intent 或 snapshot 转交新来源。
组件按原来源捕获请求，executor 在 claim/IO 前核对；独立展示可用 None，但不授予 endpoint 读取。
替换来源时创建新组件，原来源重连/刷新则保留原实例身份和 SDK 阅读事务。
设计、迁移与验收标准见[来源权限契约](rust-agent-ui-source-authority.md)。

## 可运行参考与验证

仓库内的第二 consumer 包 `tekes-agent-ui-consumer-check` 只依赖公开 AgentKit UI 包并提供针对性 API 探针；它不是
干净的外部产品依赖路径。Issue #15 增加的独立 package 样例直接依赖 `tekes-agent-components-ui`，构造并挂载
`RunPort`、`Sidebar`、`SidePanel`，不导入 demo、RunSurface 或 consumer-check。两种验证的证据层级分别记录，
未执行的外部 Cargo 构建不能记为通过。组件包负责实际业务实现。
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

旁白跨记录选择使用共享 `TranscriptNarrationPolicy`，输入必须是捕获的实际可见正文 node texts，
不是工具结果、reasoning、JSON 长度或全文字节数。`TranscriptNarrationTimeline` 的选择来自共享 Activity facts；
BetweenInputs 使用 running 时间线路径。当前 prose 适配由 Kit 读取原 OutputMessage part facts，
未知富内容或无 frame 来源时保留原正文；宿主不要另写 carrier 或补造 group fallback。
规范等价比较新增精确依赖 `unicode-normalization =0.1.25`，SDK pin 不变。
多记录切换由 Kit 的真实 NarrationFrame 根控件作为 SDK reading item；`row_id` 是阅读行，
`message_id` 是原 canonical 控件，两者可能不同。不要用阅读 span 代替复制/来源身份。
Run 和只读 pane 的组件/CoreText 验证见[frame 阅读范围](rust-agent-ui-narration-frame-reading-20261023.md)；
当前原生、富内容和逐 call 分区仍未验收。

嵌入工具的连续组现在先检查 `TranscriptSection::has_input_between_tools` 的已证明输入边界。
None 保持未知来源；该查询不是完整连续性判定，也不能代替父时间线逐 call 分区。
原消息级 Tools fragment 的物理拆分现已接入，见下文与[原工具组物理分区](rust-agent-ui-physical-tool-fragments-20261023.md)。
[来源输入边界](rust-agent-ui-tool-input-boundary-20261023.md)保留此前阶段范围，不作为当前“尚未拆分”的接入结论。
该新 API 尚未包含在 v0.3.8 中，开发宿主必须固定包含它的完整 AgentKit revision。

消息内原工具组现在通过 `MessageToolFragment(owner, OutputMessageToolFragmentKey)` 独立挂载，
每组需要一个 namespace，原 prepared plan 负责容量、身份预测及完整节点归还。旧 `MessageFragment(.., Tools)`
仅兼容第一个当前分区；穷尽 match 与工具集合遍历应消费新 key。唯一 call 的工具 key 不随 block index 变化。
有明确较早调用来源的迟到引用，继续使用原 ToolActivity，Transcript 不再重复生成详情；独立 OutputMessage
仍可展示引用。宿主不处理去重或复制控件。详见[原工具组物理分区](rust-agent-ui-physical-tool-fragments-20261023.md)。

## 未发布增量：多 endpoint Sidebar 与 Summary pointer 门禁

现有 `SidebarBinding::new(agent, endpoint_id)` 保持单端含义。多端宿主可在原 async runtime 中显式创建
`SidebarMultiEndpointBinding::new(agent, ["native", "pi", "dsh"])`，集合中的 ID 必须已由同一 SharedAgent 注册。
构造不会安装 harness、创建会话或猜测来源。消费 `snapshot()` / `subscribe()` 和 publication 的 `input(selected)`，
将完整 `SessionAddress` 作为行身份；相同 session_id 在不同 endpoint 中保留不同的行。
`publication.endpoints` 给出各端 loaded/error/terminal_error；聚合 loaded 表示首轮已收敛，不能据此宣称所有端健康。

选中补读用聚合 `read_selected(address, selection_generation)`，回执的 `project_into` 同时检查原聚合实例、
原 endpoint 事实及现有过滤/搜索限制。行动作先 `capture_action(address, definition)`，再将一次性票据交给
`perform_action(ticket, title, rpc_id)`；替换聚合实例后旧票据被拒绝，不能按当前选中行重新猜 endpoint。
返回结果仍属于原 address；宿主按原导航请求身份处理完成，不因异步结果抢走后来选中的会话。

本阶段复用单端 observer：其终态错误会保留最后 rows/stale 与端点错误；恢复需显式替换整个聚合 binding。
尚未接 demo 的首端 Sidebar 装配，也没有跨端全局查询 API；既有单端 `project_search` 不能直接接多端 publication。
完整范围及后续门槛见[多端设计](rust-agent-ui-sidebar-multi-endpoint-20261024.md)。

Summary 的 `SummaryFacts::read` 无已知会话事实，因此从工具结果提取出的 UUID 只生成被动行。
需要可导航子代理时使用 `read_with_store`：候选必须精确匹配同端 store 中的已知 session，或唯一匹配其既有
`:<uuid>` 后缀；父自身（包括带前缀的父 ID）、unknown 与歧义都拒绝。不合成 ID、不推断家族关系，也不自动加载 child。
宿主显式输入 `SummaryFacts`/sections 时仍负责其事实来源和权限，组件不代替宿主验证外部数据。
这属于未发布行为收紧；旧 v0.3.8 或历史程序的验收不因此改变。见[Summary 审计](rust-agent-ui-summary-issue25-audit-20261024.md)。
