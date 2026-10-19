# Rust Agent UI 组件接入契约

本文件是正式业务组件的实现契约与进度入口，不是完成声明。完整交付目标保持 Composer、Transcript、Indicators、RunView、SidePanel、Sidebar，加 Native/Pi/DSH 能力验证、第二公开 consumer、原生窗口验收、review/CI/合并与阶段 tag。

源码基线：AgentKit 已发布 v0.3.8 / `f1daa0ffcf6ced41e21f351e22845455ca78006c`；TekesUI 固定公开 `431093786de47b48cd566d0d9c0eafa088b10d68`，188 输入 SHA `03721866a362b1d31261ebbbc86140eb07e9e23320871c325234a5212591213d`。以下新增 API 尚是当前分支 WIP。RunSurface 明确保留为协议/控件测试宿主，不改名充当 RunView，也不是正式组件树的包装层。

## 所有权

AgentKit 定义业务结构、展示语义、组件组合、组件输入及回调，复用现有 presentation。TekesUI 拥有编辑/撤销/选区/IME/焦点、文字排版、滚动/锚点、通用布局与渲染；AgentKit 只调用其公开接口。宿主提供窗口、总体布局、导航、产品 copy/主题、安装、凭证、路径与系统权限，并执行显式 host capability 回调。不能向宿主转移本应共享的 Agent 状态机或动作策略。

## Composer（首批实现）

`Composer` 实现 TekesUI `Component`，通过 `Content::new(ids.root, composer)` 放入任意公共 ContentHost，也可 `composer.into_host()` 独立挂载。`ComposerIds::new(base)` 占用连续 8 个 ID，宿主保证其与其他子树不冲突。可在一个窗口内用不同 ID 段挂载多个 Composer。

输入为现有 RunController 发布的 `RunSnapshot` 及显式 `ComposerAffordances`。`update` 接受同一 authority 的单调 revision；无版本和旧版本失败。整个组件生命周期绑定一个 RunController；更换 controller authority 时重新创建组件，不把另一个 controller 的局部 revision 冒充旧 authority 的后续版本。组件保留各 destination 的编辑器实例与本地撤销/选区；确认 discard 后可对非当前 destination 调用 `forget` 释放实例。

`ComposerLabels` 允许宿主提供产品 copy/本地化；`ComposerAffordances` 显式提供附件、模型、预设入口，默认不显示。入口只产生 `OpenEntry {destination, entry}`，不扫描路径、安装 provider、读取凭证或推测 Harness 能力。附件上传、model/preset 等确认写入仍经共享 runtime/presentation；内容卡片已接入，实际附件适配器、批注编辑器、创建失败专用对账 UI 和完整弹层仍未完成。

`ComposerContext` 可独立挂载，或通过 `Composer::with_context` 使用宿主明确保留的 namespace 放入编辑器上方；空内容不增加布局占位。输入为同一 authority 的 RunSnapshot，输出携带原 baseline/item 的移除/预览/编辑意图，编辑器组字期间禁用卡片操作。原子 context 更新及 `draft_context_revision` 由共享 presentation 拥有，保留最新正文/settings，并阻止 Send、内容 ABA 或旧来源回调覆盖新草稿。API/字段尚未发布；[内容接入记录](rust-agent-ui-context-20261020.md) 明确区分原生 fixture、三端规范化文本回传与未完成能力。

模型/预设新增可独立挂载的 `ComposerChoices` 内容组件，输入捕获的 destination/kind、版本化宿主 catalog、selected 值与 loading/ready/error；输出带 request、catalog revision 和原 choice 的选择/关闭回调。未知 selected 保持可见，不自动选择第一项；disabled reason 保留。控件复用 TekesUI Stack/Text/Button，不实现菜单、滚动、焦点或窗口。宿主决定面板/弹层挂载位置，并在 Composer 组字期间阻止打开/切换目标。模型入口已接入 demo 的真实目录适配器与原生内容面板；预设、附件/批注、provider-specific 开关和完整弹层仍待接入。模型选择只提交 draft settings，按钮明确标注发送时应用；[模型闭环记录](rust-agent-ui-model-integration-20261019.md) 区分原生 fixture 与三端离屏证据。

`RunBinding::begin_settings` 返回保留原 binding 的一次性 `ComposerSettingsRequest`，使用既有 selection generation 防止旧目录覆盖新选择；`commit` 仅更新现有 presentation 的 draft settings，不写入新控制器、不覆盖文字/附件/批注，也不直接调用模型 RPC。宿主仍要 fence 导航和能力、校验最新目录项，并为 Send 提供既有 `SubmissionPreparation`；保留的旧 pane 不会自行知道宿主已切换窗口。详见 [绑定说明](../ui/bindings/README.md#composer-model-and-preset-content)。

`InitialPresetChoices` 读取公开 `session.initialPresets` 或 DSH `presets.list`，用于 NewThread 创建参数；已有会话保留初始预设，不开放切换。保留自动选择、损坏原因、原配置与目录身份，不把配置文本拼成 prompt；创建前重新读取目录，校验捕获 choice，再将 `agent_preset` 交给共享 session.create。第二公开 consumer 提供独立面板组合；ComponentDemo 的交互式新会话入口、自动模式和原生 fixture 现已接通；固定 session/RPC 身份，晚结果不抢导航，创建期间原 Composer 仍属于原会话。真实三端通过同一入口的离屏验证，物理 IME 与专用失败对账 UI 仍未完成。见 [新会话接入记录](rust-agent-ui-creation-20261020.md)，此前 [预设适配记录](rust-agent-ui-initial-presets-20261020.md) 保留为阶段证据。

事件流程：

1. 宿主将原始公开平台事件交给 ContentHost，保留键盘/指针元数据。
2. 每次事件或 accessibility action 后立即 `drain_actions`，再调用 `Composer::take_intents`。必须先处理完该批回调，再更新 selection/snapshot；不得把未排出的 generic action 跨 destination 延迟投递。
3. `Edit {destination, text}` 调用 `RunController::edit_text`；这个新增原子文本补丁保留当时最新的附件、批注和设置。不要用一份旧 UI 草稿替换整份 canonical draft。
4. `Send {destination}` 立即调用现有 `prepare`，再同步 snapshot、异步 `dispatch`。宿主选择已有 PromptMode，组件不创建提交 ID、推测回执或重发。发送与停止的可用性分别来自 `snapshot.can_submit` / `run.can_cancel`，不把 running 一律当成不能 queue 的理由。
5. `Cancel {scope}` 走现有 `cancel`。未知结果沿用共享对账语义，不做自动重试。
6. 改变 destination 前先排出动作并解除旧编辑器 focus。组字未结束时 `update` 拒绝改变 destination。Enter/Submit 永不转成业务发送；Send 在组字时被禁用且 intent 层再次拦截。

确认清空使用发送时的 TextEditBaseline，走 TekesUI 受控事务；晚确认不能清除发送后新输入。不再复制一份 TextFieldContent 事件处理代码；正式 Composer 与旧测试宿主只共享一个薄的 `editor.rs` delegate。

## Indicators（首批实现）

`IndicatorFacts::read` 只读取已有 RunSnapshot 的 connection/run/actionables/queue/error，以及最近 turn 的 liveUsage 或 durable reportedUsage。不从 idle 推断 completed，不估算 token，不把 live 与 durable 累加，不把 unknown 伪造成 0；partial report 保留覆盖状态。

`Indicators` 实现 Component，可独立挂载。默认 `update` 提供语义文本；产品可用 `update_with(snapshot, formatter)` 本地化同一份事实。组件不带计时器、poll loop、重连或业务 mutation。

## Transcript 结构与视觉契约

当前 Tekes App/ClientKit 的事实依据（只读对照，未改 Swift）：

- `TekesMetalTextKit/Engine/MetalNativeTurnView.swift` 的 inputLane 将 inputBubble 约束到 trailing；气泡使用 transcriptInput 颜色和 panel 圆角，inputAside 另排在气泡之下。
- `MetalVirtualLayout.swift` 的 inputTextMaximumWidth 使用 bubble 最大宽度与列宽约束；水平/垂直 padding 来自 Spacing，而不是固定百分比宽度强行撑满。
- `TekesClientKit/TranscriptRendering/TranscriptTurnFootnote.swift` 区分 input 与 output lanes。`TranscriptMetalAppBridges.swift::inputFootnoteInformation` 对 optimistic echo 使用 pending 信息，不显示伪造的服务端时间。
- 最新 AgentKit v0.3.8 的完成回合 Fork 只使用 durable settlement 边界和共享定义；不移植旧 App 中当前共享契约未支持的“回退/编辑重跑”。

正式结构为一个由稳定 turn/item 身份组织的多块文档：

| lane | 布局/样式 | 数据与动作 |
|---|---|---|
| input | 自然内容宽度、受最大宽约束、圆角气泡、居右；附件/批注与正文分层 | 已有 TranscriptItem 和共享批注 presentation，不解析自造 prompt 格式 |
| input footnote | 气泡下方独立、尾端对齐、次级文字与通用轻量按钮 | pending/真实时间、复制等显式回调；pending 不显示虚构时间 |
| output activity | 靠左，与 output 正文在同一列；推理/工具单独内容层，可折叠 | 既有 reasoning/tool DTO，展开仅为 UI 本地状态 |
| output | 靠左、正文阅读列，不套 input 气泡 | 历史/流式文本使用相同稳定身份和 renderer，不能结算后另起重复 output |
| output footnote | 正文下方独立的次级信息与动作 | 真实用量、时间和共享 footnote action definitions；disabled reason/identity 原样保留，点击时共享层再核对能力 |

圆角、间距、字体层级、前景/背景、选中/hover/focus 使用 TekesUI 既有语义 tokens。AgentKit 只增加 Agent 角色的组合/布局约束；不复制 AppKit/Metal 自有渲染器、不引入另一套配色、硬编码视觉算法或手动像素滚动补偿。窄窗口按同一结构收缩和换行；焦点、选区和阅读位置不随流式/计时更新被抢走。

实现时以现有 Tekes App 的内容层级、对齐和布局为参考，以 TekesUI 的基础组件与语义样式为基础：input 指 Transcript 中的用户消息，不是 Composer 编辑框；不将表单输入框外观带入消息正文。优先通过组件组合、公开布局属性和 token 输入实现右对齐、最大宽度、内边距与圆角；产品主题经 host 输入传入，不覆盖基础控件的交互状态。若需要基础组件尚未公开的属性或事件，先记录到 TekesUI Issue，不在 AgentKit 内复制基础控件。

最小改动还包括保留消息分组：同一条 input 的连续正文块（段落、列表、代码等）排在同一个气泡内，附件另排；不把每个 Markdown 块拆成独立聊天气泡。对应现有 `MetalVirtualLayout.swift::appendInputSurface` 的一条消息一个正文气泡布局。历史恢复和流式回传共用正式 Transcript 的结构，不能为了演示分别实现两套视觉树。TekesUI 提供基础控件、语义样式与布局能力，AgentKit 负责上述消息角色与业务内容的组合。

正式 Transcript 的验收至少覆盖：

- 视觉对照使用同一组内容，分别检查现有 Tekes 与 Rust consumer 的宽/窄窗口、浅/深色主题：短输入、长中文、含列表/代码的单条输入、附件、流式输出、推理/工具展开及两类 footnote。核对内容层级、左右对齐、气泡宽度和留白；具体颜色、圆角与交互反馈遵循 TekesUI 语义 tokens，不为逐像素复刻覆盖基础样式。截图仅作视觉证据，阅读锚点和输入行为须另行交互验证。
- 同一条用户消息在历史恢复、optimistic echo 和服务端确认后保持同一布局，短文本不被拉成整行，长中文自然换行，input footnote 保持在气泡之外并尾端对齐。
- 历史输出与流式输出使用同一阅读列；推理、工具调用及结果有独立层级，展开收起不重建整条消息；output footnote 不混入正文或推理内容。
- 页首加载更早历史、流式追加、工具展开及宽度变化后，向上阅读的位置仍绑定原消息与局部阅读位置；只有处于跟随底部状态时追加才继续跟随。
- 会话切换分别恢复各自阅读位置；重连与历史回补不重复显示已有消息，不抢焦点或清除选区。分页的 loading/error/retry 使用共享状态，重试由用户显式触发。

当前 `RunSurface` 中将消息串成单一文本的 `History` 仅用于协议与阅读 API 测试，不满足上述布局，也不作为正式 Transcript 或 RunView 的实现基础。

### 已核实的基础阻塞

- [TekesUI #22](https://github.com/TekesApps/TekesUI/issues/22)：混合 Content 文档的稳定子项锚点、历史前插/高度变化/重排事务。现有 ScrollContent 可滚动，ReadingController 已完成单 TextSurface；这不是重开 #16，也不能用拼成单一文本框替代多块 Transcript。
- [TekesUI #23](https://github.com/TekesApps/TekesUI/issues/23)：无表单装饰的可选中文本 Component/presentation 模式，保留现有 TextSurface/AX/选择/复制/受控更新。当前 TextFieldContent 固定 form 的 label/message/边框布局；TextContent 不提供等价文本选择服务。

新 #23 公开 `61ddf7c68fccd819e20a01ba3104a8a0f8d7e64f` 的独立消费复验通过正文 API/核心/consumer 和临时 AgentKit 历史/流式探针，但公共类型注册门禁失败，暂不提升正式 pin。见 [公开输入复验](tekesui-issue23-public-review-20261020.md)。

两个 Issue 均有精确源码、所需事件/属性、独立 consumer 和验收条件。上游若给出等价公共 API，则用它并补消费证据，不重复开发。新输入必须显式升级 pin 后复验；不能跟随活跃 WIP。

#23 的未发布归档候选 `2f0902d9f90fb069696c52edc38d9b7e7f8f0d0c4a3f0141e21fe1ac3f3183d0` 已独立校验 991 个文件，并复验公开 API、控件核心和第二 consumer；详见 [候选技术评估](tekesui-plain-candidate-review-20261019.md)。这不改变正式 pin 或 #22/#23 的交付状态。

## RunView 的后续实施

RunView 组合正式 Composer / Transcript / Indicators，拥有 Agent 组件间意图路由；不拥有窗口、其他产品导航或第二套 Run 状态机。异步通知继续订阅已有 presentation，通过公开 WindowWake 唤醒。

## Sidebar（首批实现）

`Sidebar::new(namespace, labels)` 实现公共 Component，也可 `into_host()` 独立挂载。每个组件使用独立的 u32 ID namespace；输入 `SidebarInput` 提供单调 revision、按消费方分组/排序偏好排列的 rows 和当前 SessionAddress。

每行直接提供共享服务的 ThreadActivity、ThreadPinPresentation 与 ThreadActionsPresentation。组件检查动作的 endpoint/session 与行身份一致，不从标题、编码 ID、是否有运行来猜权限/完成状态；置顶关系不决定显示顺序、不写 UI 自有存储。选择回调携带完整 SessionAddress，同名 session 在不同 endpoint 中保持独立。

选中行使用 TekesUI ButtonContent 已有的 selected 属性、quiet 样式及语义。当前把所选会话的共享动作显示为侧栏操作区；按共享顺序显示标签与 disabled reason，不新写菜单/按钮状态机。完整产品的菜单外观、键盘和视觉仍待原生验收。

`take_intents` 只产生 `Select` 或 `Action {address, definition}`。copy/rename 保留原始 interaction/copyText；执行经已有 SharedAgent 的 thread.performAction，使用同一个 RPC identity 显式对账，不能自动重试。UI 回调不是执行许可：共享 dispatcher 必须按当时的新鲜能力重新核对。普通状态更新/排序保留原按钮实例；移除后再加入的项目不复用已退休的 UI action ID。

验证已包含真实 SharedAgent + 独立 SEP fixture + ThreadPreferences：一次 pin、同 RPC 重放、再次读取共享状态与重连均通过。该用例不代表三端原生窗口已验收。

## SidePanel 与 Summary（首批实现）

`SidePanel` 是 Agent 业务 tab/pane 容器，不再包一层 RunSurface。输入 `SidePanelInput` 提供父 RunScope、单调 revision、打开的 SidePanelTab、当前选择与现有 SideChatSnapshot。task tab 来自 AgentTask；subagent tab 必须显式提供 ChildHistoryLayout；SideChat tab 来自实际 SideChat。`supports_side_chat` 使用现有 controller 的能力结果：Some(true)/Some(false)/None 分别为支持/不支持/尚无结论，不将未知显示成不支持。

选择回调携带原始 parent 和完整 tab 定义，包括子历史策略。关闭与创建只发作用域明确的意图，不自行 fork/discard/cancel。SideChat unknown/closing 时禁用再次关闭，创建 pending/unknown 时禁用再次创建并呈现共享错误；在共享 controller 仍持有同一临时 pointer 时，组件拒绝从输入中移除该 tab。确认 discard 后才能移除，不能用隐藏 UI 冒充关闭成功。

pane 内容用独立 namespace 的 Kit Component 组合：

1. `update` 打开/选择 tab 后获取 `content_token(kind)`。
2. 数据首先通过现有 TaskPanelState 的 commit/generation 或 ChildRun / SideChatController 校验；subagent 仍用明确的 bounded/independent adapter，不能回退为父历史。
3. 将实际 Kit 内容包装为公开 `Content`，通过 `install_content(token, content)` 放入对应 pane。上游正式 Transcript 尚未可消费时，未挂载位置只是明确的待接入位置，不能算子运行内容完成。
4. 同一 tab 的普通数据更新保留组件实例；切换 tab/父 scope 后缓存精确实例，返回恢复编辑/阅读的控件状态。缓存有显式父 scope 数量上限，淘汰只释放 UI/观察，不取消运行或偷偷 discard。
5. PanelContentToken 只约束 UI mount 生命周期，不能代替业务 load token。A→B→A、关闭重开、history layout 改变、父缓存淘汰会使旧 token 失效。

宿主在结构切换前排出旧动作并通过公共 ContentHost 解除旧 focus、结束/取消组字；不能带着预编辑状态直接换走活动 pane。窗口级总体导航归宿主，pane 组件不另建业务状态机。tab strip 当前由基础 ButtonContent 组合；选中语义仍来自其现有 selected 属性，完整原生 tab 交互与视觉尚未验收。

`SummaryView` 是实际可独立挂载的内容组件。`SummaryInput` 接收现有 SummaryFacts（与 scope 严格匹配）和显式 host sections。章节、membership、总量、cost 覆盖及动作直接来自 SummaryFacts.sections / append_summary_sections；渲染层不重新计算业务规则。已知 0 保留，成本不完整保持未知。`SummaryIntent` 携带原 scope 与原 SummaryAction，产物交显式宿主能力打开，不直接读文件。SummaryView 已在真实 SidePanel 内容 slot 中验证动作回传。

任务详情、子代理与 SideChat 的正式 Run 内容仍要由正式 RunView 接上，当前没有把占位文案当成这些业务视图交付。既有 TaskPanelState、ChildRun、SideChatController 继续是状态权威，不迁移进 tab 容器。

## Runtime 绑定（首批实现）

独立 `tekes-agent-ui-bindings` 包提供 `RunBinding`，只连接现有 runtime/presentation 与组件生命周期。详见 [绑定接入说明](../ui/bindings/README.md)。主会话、已解析任务、独立子代理和 SideChat 保留各自历史观察；Native 同 session 有界子历史只返回只读 Transcript，不伪造主会话 Composer，也不回退父历史。独立子代理必须传直接 parent；嵌套 parent 的 receiving session 不等同于导航 thread root。

SideChat 的创建/关闭/取消至 Unknown、显式 reconciliation 均通过现有 controller 的新订阅通知；读取 snapshot 和失败的无变化查询不触发通知。绑定按当前 pointer 归属计算控件 gate，发送在 admission 前和宿主 preparation 返回后重新检查；pointer 移除后返回 Missing，宿主需卸载 pane body。该绑定不代替正式 RunView；主会话绑定已接入 `--components` 原生 Composer/Indicators 验证窗口，SidePanel Run 内容尚未接入。

Task tab 以 task ID 为稳定身份，scope 是可更新的内容输入。TaskPanelState 的 loading/missing/error 原样传入；pointer 更新保持 header ID，但淘汰旧 body/mount token，旧结果不得覆盖新源。

本轮本地验证：root 104、UI 62、工具 18 tests，root/UI strict all-targets Clippy、fmt、边界和 Cargo 实际解析的 188 个 TekesUI 输入检查通过。UI 含 5 个绑定集成测试和第二公开 consumer。只证明本地组件/runtime 路径，不证明正式组件树的原生窗口或三端 UI 验收；[验证范围收据](rust-agent-ui-bindings-20261019.json)。上游 main 的后续 PR #24 仅涉及 CI/测试/Gallery，不提供 #22/#23 所需公共运行时 API；当前 pin 保持不变。

## 完成账本

最新 [子代理 pane 接入记录](rust-agent-ui-child-20261019.md) 覆盖 typed 子源、直接 parent、独立 Composer/Indicators、有界只读加载与分页诊断、复用与撤销、单次读取及过期结果处理。本地 UI 增至 83 tests；原生 fixture 覆盖两条路径，但不是正式子 Transcript，也不是三端子 pane 的完整原生交互验收。

最新 [任务 pane 接入记录](rust-agent-ui-task-20261019.md) 覆盖 TaskPanelState 绑定、源指针替换、异步发送撤销、独立 Composer/Indicators 与原生 fixture 验证。本地 UI 增至 79 tests。显式宿主任务源与既有 Summary OpenTask 动作可接入，同会话 todo 的无独立 run 导航语义保持不变；仍不代表任务正式 Transcript 或子代理 pane 完成。

最新 [SideChat 接入记录](rust-agent-ui-sidechat-20261019.md) 与 [机器收据](rust-agent-ui-sidechat-20261019.json) 覆盖共享 pointer 绑定、独立 Composer/Indicators、创建/发送/取消关闭、Unknown 保留与父草稿隔离。本地 root 104、UI 74、工具 18 tests 通过，三端协议与组件到 GPU 路径复验通过；正式 Transcript 与完整原生交互仍不在这些通过项内。

最新 [Sidebar 操作接入记录](rust-agent-ui-thread-actions-20261019.md) 覆盖 ThreadRename、分叉导航、归档空选择与子代理 owner 归档刷新。基线 v0.3.8；root 105、UI 88、工具 18 tests 和局部原生回归通过，正式 Transcript/RunView 仍未完成。

| 要求 | 当前状态 | 还需要的证据 |
|---|---|---|
| Composer 独立输入/草稿/发送/取消及入口 | 基础组件、模型闭环、内容卡片/原子移除；UI 100 tests 与局部原生/三端离屏验证 | 实际附件适配器、批注编辑/来源高亮/内容 undo、预设与推理开关、完整弹层/三端原生验收 |
| Indicators | 首批事实组件与 unknown/0/partial/error 测试 | 原生 RunView 呈现、主题/语义验收 |
| Transcript / RunView | 布局契约已核实，正式组件未交付 | #22/#23 等价公共 API、结构化实现、阅读/分页/流式/原生验收 |
| Sidebar | 共享服务绑定、选择/置顶/复制；重命名/分叉/归档已接入，局部原生 fixture 验证 | 真实三端操作与完整宿主验收 |
| SidePanel / Summary | Summary 原生挂载；任务、SideChat、独立子代理控制与绑定已接入；有界子历史只读加载诊断/分页已验证 | 各类 pane 正式 Transcript、真实三端子 pane 原生操作与完整联调 |
| 第二公开 API consumer | Composer、Indicators、Sidebar、SidePanel 公共组合，Summary 独立挂载；不依赖 RunSurface | 正式 RunView/Transcript 与完整原生宿主组合验证 |
| 原生窗口验收 | Composer/Indicators、Sidebar、Summary + 共享绑定的局部窗口验证已通过（合成文本服务） | 物理中文输入法、正式 RunView/Transcript/其余 SidePanel 内容和阅读/分页锚点验收 |
| review / CI / merge / tag | 草稿 PR #10 已提交；先前提交 CI 通过，后续增量仍需各自验证 | 完整要求审计、独立 review、最新 CI、合并与新阶段 tag |

既有三端 test-harness 到 GPU 记录继续有效，但它不是正式业务组件树的原生窗口验收，也不是以上全部组件已完成的证据。目标保持 active，不因初步测试通过提前收口。

最新的 [Composer/Indicators 原生接入记录](rust-agent-ui-native-controls-20261019.md) 进一步验证了实际公共组件与绑定的原生窗口和三端数据到 GPU 路径。该证据与原 RunSurface 测试路径分开，不提升尚未实现的 Transcript/RunView 或物理 IME 验收状态。

首批本地 gate（2026-10-19）：主 workspace 103 tests、UI workspace 42 tests（其中新增 4 个正式 Composer controller 用例、1 个 Indicators 用例、1 个第二 consumer 挂载用例），root/UI strict all-targets Clippy、fmt 与依赖边界通过。旧测试宿主在共享 delegate 提取后仍可构建并生成 GPU PNG；该图不证明正式组件原生窗口已验收。当前代码仍未提交/发布，TekesUI 的固定 188 输入重查一致。

本轮 SidePanel/Sidebar/Summary gate（2026-10-19）：UI workspace 56 tests（新增 Sidebar 4、SidePanel 5、Summary 3、独立 consumer 2），主 workspace 103 tests、Python 工具 18 tests；root/UI strict all-targets Clippy、fmt 与边界检查通过。新的 macOS UI consumer CI job 已声明，会构建 demo、运行组件/第二 consumer 测试，并校验 Cargo 实际解析的 8 个 UI package / 188 个文件。工作流尚未提交触发，不能称远程 CI 已绿。实际解析输入检查还拒绝错误 Git 来源、路径逃逸、变动/新增文件和 symlink。机器证据见 [本轮收据](rust-agent-ui-panels-20261019.json)。
