# Rust Agent UI 组件接入契约

本文件是正式业务组件的实现契约与进度入口，不是完成声明。完整交付目标保持 Composer、Transcript、Indicators、RunView、SidePanel、Sidebar，加 Native/Pi/DSH 能力验证、第二公开 consumer、原生窗口验收、review/CI/合并与阶段 tag。

[原回合导航契约](rust-agent-ui-turn-navigation-20261021.md)增加非抢焦点的捕获/一次 dispatch 与 pane ABA 拒绝；#28 的 nearest/centered 在真实 RunView 中单独验证。普通回合顶端对齐仍缺 TekesUI [#32](https://github.com/TekesApps/TekesUI/issues/32)，正式入口未接通，不以居中替代。

[Summary 用量两列布局](rust-agent-ui-summary-usage-20261021.md)将六项用量的标题与数值拆成原 TekesUI 文本槽，增加公开格式化回调；不重算 facts 或切换默认依赖。完整 Summary rows/slots 与原生产品验收仍待完成。

源码基线：AgentKit 已发布 v0.3.8 / `f1daa0ffcf6ced41e21f351e22845455ca78006c`；TekesUI 实验开发输入固定公开 `8cf0840790334ba224afc0595897c74231c712e6`，199 输入 SHA `f0af230a1efe97334b90eb193f77ca0d30dec43ef09e487ad6948c38a938257f`，原 431 基线清单保留在 `ui/baselines/`。以下新增 API 尚是当前分支 WIP。RunSurface 明确保留为协议/控件测试宿主，不改名充当 RunView，也不是正式组件树的包装层。

Layout 和显示逻辑须逐项严格对齐当前 ClientKit 对应 Views；[对应关系与初步差异](clientkit-view-layout-review-20261020.md)列出源码及待验收项。组件类型存在、协议通过或 TekesUI 基础样式一致均不等于布局已经对齐。[严格共享列修正](rust-agent-ui-strict-column-20261021.md)已补源320/770规则、token推导净空和比例归一化；基础文档/Composer叠层现已推进，详见[underlay与阅读净空](rust-agent-ui-overlay-20261021.md)；完整rail、selection/Jump及其它清单仍未完成。[Composer显示规则](rust-agent-ui-composer-visibility-20261021.md)现已补归档隐藏、中断/暂不可提交保留、原实例与原生归档恢复专项；后续[降级快照修正](rust-agent-ui-degraded-frame-20261021.md)补齐坏历史期间的归档、epoch和目标隔离。

## 所有权

[Sidebar 首次恢复与观察失败](rust-agent-ui-sidebar-restoration-20261021.md)现已贯通 shared inventory 就绪事实、binding 和公开组件输入；未恢复不再显示确认空，观察失败保留最近列表。搜索/分组和完整 outline/status-pane 布局仍待继续。

[Summary 标题槽与 #31 候选消费](rust-agent-ui-summary-label-slot-20261021.md)记录默认库组件组合与隔离 label API 验证；默认 pin 的省略能力仍待公开版本。

[布局激活与成功帧边界](tekesui-layout-text-candidate-20261021.md)记录 #27 的独立候选消费。`render` 成功且原业务 publication/epoch 匹配后，宿主才发布当前原生文本、AX、几何和可见性；失败帧不靠更换编辑器或重试 Agent 操作恢复。

### 排版与基础样式

[Summary section 顺序与折叠](rust-agent-ui-summary-sections-20261021.md)现已补齐宿主优先顺序、按 section ID 保留的折叠、真实隐藏的输入/AX 子树及任务 pointer 改变时的旧动作拒绝。公开 DTO/查询支持第二 consumer；单行省略见 TekesUI #31，完整 Summary rows/custom slots 与整体视觉仍未验收。

[Composer 输入行与单一主动作](rust-agent-ui-composer-band-20261021.md)已按对应 View 的内容顺序推进：正文与主动作/Add 共用输入 band，发送/停止互斥并保留原控件，空 setup 不占位，十行上限随字体倍率更新。模型/预设的宿主 toolbar、完整 palette/chip/notice 与 glyph slot 仍待对齐；默认 TekesUI 样式和固定 pin 不变。

[回合 rail 的真实目标与布局输入](rust-agent-ui-turn-rail-20261021.md)已公开：源回合到原内容行的映射、实际排版可见性、原 owner/publication/epoch 校验，以及对应 ClientKit 的 gutter/track 几何。该几何是计划槽位，尚非可交互 rail 子树；基础 reveal 和紧凑 action 需求分别见 TekesUI #28/#29，实际 hover/预览/跟随与完整组合仍由 AgentKit 完成。

AgentKit 固定消息排版语义：机器正文、推理、工具内容及机器 footnote 靠左；用户输入使用靠右的圆角气泡，用户 footnote 在气泡外靠右。主会话和各类子 pane 共用此规则。

基础视觉跟随 TekesUI：优先使用公开组件的默认样式，必要的语义间距和文字层级使用库提供的 tokens。AgentKit 不另建颜色、字体、圆角或控件皮肤，也不为复刻 Tekes App 的旧外观覆盖库默认值。宿主主题通过 TekesUI 的公开主题输入传递。

升级精确固定的 TekesUI 版本后，其基础样式调整应在所有 Agent 组件中统一体现，无需逐个组件补丁；消息方向、footnote 归属和业务行为继续保持。升级验收检查浅色/深色、长短消息、换行和窄窗口下的对齐及可读性，不锁定旧版具体像素值。缺少基础组件属性或事件时向 TekesUI 提 issue，不在消费侧复制通用实现。

最新 Transcript 审查修复及分层验证见[非法历史隔离与空容器记录](rust-agent-ui-transcript-review-20261020.md)：公开只读组件支持缓存替换前预检，非法新历史保留原内容并允许显式刷新；明确为空的 Assistant 容器不显示缺失正文提示。这不提升阅读锚点、富内容或真实 IME 的验收状态。

后续[系统拼音软件探针](rust-agent-ui-system-ime-20261020.md)已通过真实系统 marked text、发送/导航屏障、确认及单次提交检查；按键定向合成到隔离进程，不注入 Preedit。此证据不提升实体键盘、候选窗、完整 IME 或完整三端原生体验验收状态。

用户消息的[外置复制入口](rust-agent-ui-input-copy-20261020.md)已接入 RunView 与只读 lane，返回捕获原 scope/message/text 的公开 intent，由宿主写剪贴板。[输出 footnote 复制](rust-agent-ui-output-copy-20261020.md)复用共享 is_final 语义，只为已完成回合中唯一的最终正文提供目标；评价等其余 footnote 语义仍待补齐。离线复制不授予 endpoint mutation 权限。

[评价读取基础](rust-agent-ui-feedback-read-20261020.md)已增加 committed records、真实目标身份和原 binding/连接/读顺序约束；DSH feedback 包裹已在适配器边界归一化。[评价 CAS 写入基础](rust-agent-ui-feedback-cas-20261020.md)已补齐原目标捕获、一次性 mutation、备注策略和权威冲突恢复，并通过三端真实读写；[独立评价控件与备注编辑器](rust-agent-ui-feedback-controls-20261020.md)现已提供公开 API 和原 binding gesture bridge；现已[挂入正式 Transcript/RunView 的输出 footnote](rust-agent-ui-feedback-transcript-20261020.md)，包括原 binding 路由、组字/导航屏障及延迟回执隔离；[三端评价专项原生窗口验收](rust-agent-ui-feedback-native-20261020.md)现已补齐自动化点击和原生文本服务路径；完整 Transcript、物理输入法与阅读锚点的证据范围仍单列。

AgentKit 定义业务结构、展示语义、组件组合、组件输入及回调，复用现有 presentation。TekesUI 拥有编辑/撤销/选区/IME/焦点、文字排版、滚动/锚点、通用布局与渲染；AgentKit 只调用其公开接口。宿主提供窗口、总体布局、导航、产品 copy/主题、安装、凭证、路径与系统权限，并执行显式 host capability 回调。不能向宿主转移本应共享的 Agent 状态机或动作策略。

[Pi 实时预览与落盘身份](rust-agent-ui-stream-identity-20261020.md)补齐 Pi 1.3 delta-only RPC 的公开 extension 归一化及 Stream→NativeEntry 明确回执。正式 Transcript 保留原输出行与文本控件；完整阅读锚点、富内容和三端原生验收范围不变。

[三端流式原生专项](rust-agent-ui-stream-native-20261020.md)已验证正常 Send、真实预览、落盘后的原行/正文控件/选区、会话草稿和重连不重发，并修复聚合/结构化回补时的唯一正文/推理控件迁移；多块内容不猜测关联。这是专项原生证据，完整三端取消/错误/IME 交互及阅读锚点仍须继续。

[三端取消/错误/恢复专项](rust-agent-ui-run-controls-20261020.md)已通过真实窗口 Send、流式中 Cancel、两次重连不重发、公开错误说明的原生 AX 查询及显式恢复。新增共享 latest_failure/failure_message 只读事实，修复 Pi 错误正文丢失、Indicators 只显示 idle，以及 DSH 新执行后旧 operational error 粘滞。基础路径已验收，不扩大为全部错误、队列/工具取消或真实 IME 验收。

[阅读事务候选输入](tekesui-reading-candidate-intake-20261020.md)已核验新包的449文件与压缩包身份；后续[独立回放与正式 Transcript 探针](rust-agent-ui-reading-candidate-20261020.md)已验证预检行身份、追加/前插、A/B 恢复和保留焦点的回到最新。默认8cf固定依赖未替换，完整 RunView 与原生阅读仍待完成，不据此提升/关闭#22。

[RunView/Composer context 预检修复](rust-agent-ui-context-preflight-20261020.md)防止卡片拒绝后正文部分安装，已通过完整 UI 及正式组件树原生 fixture 专项。完整受控编辑器与阅读事务的可组合预检交由 [TekesUI #26](https://github.com/TekesApps/TekesUI/issues/26)，已有精确公开 API 复现；未在消费侧复制通用校验。

[共享会话列](rust-agent-ui-column-layout-20261020.md)现已接入正式 RunView，提供居中限宽/全宽/非对称边距并保留编辑器和消息身份；完整叠层和回合 rail 仍未完成。

[#26 新隔离候选](tekesui-controlled-preflight-candidate-20261020.md)已通过接收方463输入核验、388 controls/168 consumer/严格 Clippy/48场景192帧回放；尚非正式 AgentKit RunView 或原生阅读接入，也不替换默认公开 pin。

[正式 RunView 整帧准备与候选阅读事务](rust-agent-ui-run-preflight-20261021.md)提供只读 Agent 参数计划、事务前计划重检与提交重检；七项正式组件候选检查覆盖合法/拒绝更新、旧 token、ABA、迟到清空、组字保留及前插位置。默认宿主与三端原生阅读尚未迁移，未替换公开固定 pin。

## Composer（首批实现）

`Composer` 实现 TekesUI `Component`，通过 `Content::new(ids.root, composer)` 放入任意公共 ContentHost，也可 `composer.into_host()` 独立挂载。`ComposerIds::new(base)` 占用连续 8 个 ID，宿主保证其与其他子树不冲突。可在一个窗口内用不同 ID 段挂载多个 Composer。

输入为现有 RunController 发布的 `RunSnapshot` 及显式 `ComposerAffordances`。`update` 接受同一 authority 的单调 revision；无版本和旧版本失败。整个组件生命周期绑定一个 RunController；更换 controller authority 时重新创建组件，不把另一个 controller 的局部 revision 冒充旧 authority 的后续版本。组件保留各 destination 的编辑器实例与本地撤销/选区；确认 discard 后可对非当前 destination 调用 `forget` 释放实例。

`ComposerLabels` 允许宿主提供产品 copy/本地化；`ComposerAffordances` 显式提供附件、模型、预设入口，默认不显示。入口只产生 `OpenEntry {destination, entry}`，不扫描路径、安装 provider、读取凭证或推测 Harness 能力。附件上传、model/preset 等确认写入仍经共享 runtime/presentation；内容卡片已接入，通用文件上传共享绑定已实现，见 [附件接入记录](rust-agent-ui-attachments-20261020.md)；主 Composer 的系统选择器/上传状态已接入，见 [宿主接入记录](rust-agent-ui-upload-host-20261020.md)；草稿通用文件预览已接入，见 [预览记录](rust-agent-ui-file-preview-20261020.md)；历史/图片预览、失败对账、来源定位/高亮、创建失败专用对账 UI 和完整弹层仍未完成。

`ComposerContext` 可独立挂载，或通过 `Composer::with_context` 使用宿主明确保留的 namespace 放入编辑器上方；空内容不增加布局占位。输入为同一 authority 的 RunSnapshot，输出携带原 baseline/item 的移除/预览/编辑意图，编辑器组字期间禁用卡片操作。原子 context 更新及 `draft_context_revision` 由共享 presentation 拥有，保留最新正文/settings，并阻止 Send、内容 ABA 或旧来源回调覆盖新草稿。API/字段尚未发布；[内容接入记录](rust-agent-ui-context-20261020.md) 明确区分原生 fixture、三端规范化文本回传与未完成能力。

模型/预设新增可独立挂载的 `ComposerChoices` 内容组件，输入捕获的 destination/kind、版本化宿主 catalog、selected 值与 loading/ready/error；输出带 request、catalog revision 和原 choice 的选择/关闭回调。未知 selected 保持可见，不自动选择第一项；disabled reason 保留。控件复用 TekesUI Stack/Text/Button，不实现菜单、滚动、焦点或窗口。宿主决定面板/弹层挂载位置，并在 Composer 组字期间阻止打开/切换目标。模型及公开推理档位、新会话初始预设、Main 通用附件选择/上传状态及已有批注评论已接入；预览/来源选区捕获、provider-specific 开关和完整弹层仍待接入。模型选择只提交 draft settings，按钮明确标注发送时应用；[模型闭环记录](rust-agent-ui-model-integration-20261019.md) 区分原生 fixture 与三端离屏证据。

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

`AnnotationEditor` 通过当前卡片的 EditAnnotation 意图编辑已有评论，已提交输入同步至共享草稿；不改原文、source/locator，不自行解析 prompt。原 controller snapshot 确认同时推进 context 与 publication 屏障，保留 TekesUI 字段/选区/undo；失效时保留本地文字，不自动重放。见 [评论编辑接入](rust-agent-ui-annotation-editor-20261020.md)。

## Indicators（首批实现）

`IndicatorFacts::read` 只读取已有 RunSnapshot 的 connection/run/actionables/queue/error，以及最近 turn 的 liveUsage 或 durable reportedUsage。不从 idle 推断 completed，不估算 token，不把 live 与 durable 累加，不把 unknown 伪造成 0；partial report 保留覆盖状态。

`Indicators` 实现 Component，可独立挂载。默认 `update` 提供语义文本；产品可用 `update_with(snapshot, formatter)` 本地化同一份事实。组件不带计时器、poll loop、重连或业务 mutation。

## Transcript 结构与视觉契约

统一方向规则（与当前 Tekes App 对齐）：机器产生的正文、推理、工具、附件、会话提示及对应 footnote 全部靠左；用户输入、用户附件和用户 footnote 全部靠右。用户 footnote 在气泡外，与气泡尾端对齐；机器 footnote 与正文阅读列左边缘对齐。方向按所属消息 lane 判断，用户的送达/失败状态仍随用户 footnote 靠右，独立机器提示靠左。该规则统一用于主会话、任务、子代理和 SideChat，不能在 pane 中另设一套消息方向。这里约束正式视图；现有只读模型与测试宿主不构成视觉验收。

已有 [只读消息分层](rust-agent-ui-transcript-document-20261020.md)：公共 TranscriptDocument 从既有 snapshot 组织 input/output/activity/footnote、保留身份和覆盖事实。它不是渲染组件，正式 Transcript / RunView 的上游门禁与验收范围不变。

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
| output footnote | 靠左，与正文阅读列左边缘对齐；正文下方独立的次级信息与动作 | 真实用量、时间和共享 footnote action definitions；disabled reason/identity 原样保留，点击时共享层再核对能力 |

视觉样式以 TekesUI 的组件默认值、当前主题与语义 tokens 为统一来源，AgentKit 只保留消息对齐、气泡归属和内容层级等业务排版约束。未来 UI 库整体调整时，通过显式升级固定依赖版本统一体现，不为 AgentKit 再维护一份配色、字体或控件样式数值表；现有控件渲染继续接收当前主题输入。圆角、间距、字体层级、前景/背景、选中/hover/focus 使用 TekesUI 既有语义 tokens。AgentKit 只增加 Agent 角色的组合/布局约束；不复制 AppKit/Metal 自有渲染器、不引入另一套配色、硬编码视觉算法或手动像素滚动补偿。窄窗口按同一结构收缩和换行；焦点、选区和阅读位置不随流式/计时更新被抢走。

实现时以现有 Tekes App 的内容层级、对齐和布局为参考，以 TekesUI 的基础组件与语义样式为基础：input 指 Transcript 中的用户消息，不是 Composer 编辑框；不将表单输入框外观带入消息正文。优先通过组件组合、公开布局属性和 token 输入实现右对齐、最大宽度、内边距与圆角；产品主题经 host 输入传入，不覆盖基础控件的交互状态。若需要基础组件尚未公开的属性或事件，先记录到 TekesUI Issue，不在 AgentKit 内复制基础控件。

最小改动还包括保留消息分组：同一条 input 的连续正文块（段落、列表、代码等）排在同一个气泡内，附件另排；不把每个 Markdown 块拆成独立聊天气泡。对应现有 `MetalVirtualLayout.swift::appendInputSurface` 的一条消息一个正文气泡布局。历史恢复和流式回传共用正式 Transcript 的结构，不能为了演示分别实现两套视觉树。TekesUI 提供基础控件、语义样式与布局能力，AgentKit 负责上述消息角色与业务内容的组合。

正式 Transcript 的验收至少覆盖：

- 主题继承单独验收：同一份消息与同一组件实例切换宿主主题时，基础控件的颜色、字体与交互状态跟随 TekesUI；显式升级固定 TekesUI 版本后，其默认样式调整应统一体现在 Agent 组件中。AgentKit 不以局部样式覆盖锁住旧外观，左右对齐、消息分组与外置 footnote 等业务排版约束继续成立；样式更新不重置草稿、组字、选区或阅读位置。
- 视觉对照使用同一组内容，分别检查现有 Tekes 与 Rust consumer 的宽/窄窗口、浅/深色主题：短输入、长中文、含列表/代码的单条输入、附件及无正文的纯附件输入、流式输出、推理/工具展开及两类 footnote。核对内容层级、左右对齐、气泡宽度和留白；具体颜色、圆角与交互反馈遵循 TekesUI 语义 tokens，不为逐像素复刻覆盖基础样式。截图仅作视觉证据，阅读锚点和输入行为须另行交互验证。
- 同一条用户消息在历史恢复、optimistic echo 和服务端确认后保持同一布局，短文本不被拉成整行，长中文自然换行，input footnote 保持在气泡之外并尾端对齐。纯附件输入保持可见，不能因正文为空而丢弃附件或对应 footnote。
- 历史输出与流式输出使用同一阅读列；推理、工具调用及结果有独立层级，展开收起不重建整条消息；output footnote 不混入正文或推理内容。
- 页首加载更早历史、流式追加、工具展开及宽度变化后，向上阅读的位置仍绑定原消息与局部阅读位置；只有处于跟随底部状态时追加才继续跟随。
- 会话切换分别恢复各自阅读位置；重连与历史回补不重复显示已有消息，不抢焦点或清除选区。分页的 loading/error/retry 使用共享状态，重试由用户显式触发。

当前 `RunSurface` 中将消息串成单一文本的 `History` 仅用于协议与阅读 API 测试，不满足上述布局，也不作为正式 Transcript 或 RunView 的实现基础。

### 当前基础能力与阻塞

- [TekesUI #22](https://github.com/TekesApps/TekesUI/issues/22) 尚待交付：混合 Content 文档的稳定子项锚点、历史前插/高度变化/重排事务。现有 ScrollContent 可滚动，ReadingController 已完成单 TextSurface；不能用拼成单一文本框替代多块 Transcript。
- [TekesUI #25](https://github.com/TekesApps/TekesUI/issues/25) 新增：可选中富内容 Component、受控流式更新与原生文本映射。现有 MarkdownView 提供有界 Markdown 被动绘制，不能据此宣称已有可选中富内容组合接口。当前源码文字显示和显式原始数据详情保留内容，富内容样式继续待接入。
- #23 的无表单只读 Content API 已由当前固定 8cf 后继实际消费。原61dd候选的注册失败保留在[历史评估](tekesui-issue23-public-review-20261020.md)，后继的通过证据见[独立复验](tekesui-issue23-ci-review-20261020.md)，不再列作当前纯文本阻塞。

基础能力由 TekesUI 实现；收到公开不可变版本后显式升级并验证，不跟随活跃 WIP。AgentKit 自身仍需完成消息组合、stream 到 durable 的可靠阅读身份关联、其余 footnote 动作、正式 Transcript/RunView 和 pane 消息接入，不把这些工作归为上游阻塞。

## RunView（实验组合已实现）

RunView 组合正式 Composer / Transcript / Indicators，拥有 Agent 组件间意图路由；不拥有窗口、其他产品导航或第二套 Run 状态机。异步通知继续订阅已有 presentation，通过公开 WindowWake 唤醒。

当前公开 [`RunView`](rust-agent-ui-run-view-20261020.md) 已组合 Composer、Indicators、分页、问答和可安装的混合内容树，提供同帧 update 与类型化意图出口。实验主会话和可写 pane 已复用它，第二 consumer 可独立构造；RunSurface / ComponentDemo 仍仅作测试宿主。完整 Transcript/RunView 验收尚未通过：可靠 stream-to-durable 关联、#22 阅读/分页锚点、#25 富内容及物理 IME/完整三端原生 UX 仍需完成。

## Sidebar（首批实现）

[搜索 footer](rust-agent-ui-sidebar-search-footer-20261021.md)现已接入独立显示输入、底部锚定、长错误下 Retry 可见性及捕获原请求的公开回调。[共享单端查询控制器](rust-agent-ui-sidebar-query-controller-20261021.md)现已接入 presentation 与公开 RPC；[未知命中的公开库存恢复](rust-agent-ui-sidebar-inventory-refresh-20261021.md)已补齐；跨端聚合、搜索成员关系和完整 outline 仍待完成，不由宿主另写业务状态机。

`Sidebar::new(namespace, labels)` 实现公共 Component，也可 `into_host()` 独立挂载。每个组件使用独立的 u32 ID namespace；输入 `SidebarInput` 提供单调 inventory revision、loaded/error、按消费方分组/排序偏好排列的 rows 和当前 SessionAddress。首次恢复前不显示确认空；观察失败保留已恢复列表。选择来自独立宿主导航，可在同一库存 revision 下变化。

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

SideChat 的创建/关闭/取消至 Unknown、显式 reconciliation 均通过现有 controller 的新订阅通知；读取 snapshot 和失败的无变化查询不触发通知。绑定按当前 pointer 归属计算控件 gate，发送在 admission 前和宿主 preparation 返回后重新检查；pointer 移除后返回 Missing，宿主需卸载 pane body。该绑定不代替正式 RunView；主会话及任务、独立子代理、SideChat 绑定已接入 `--components` 原生 Composer/Indicators 验证窗口；各 pane 的正式 Transcript/RunView 尚未接入。

Task tab 以 task ID 为稳定身份，scope 是可更新的内容输入。TaskPanelState 的 loading/missing/error 原样传入；pointer 更新保持 header ID，但淘汰旧 body/mount token，旧结果不得覆盖新源。

本轮本地验证：root 104、UI 62、工具 18 tests，root/UI strict all-targets Clippy、fmt、边界和 Cargo 实际解析的 188 个 TekesUI 输入检查通过。UI 含 5 个绑定集成测试和第二公开 consumer。只证明本地组件/runtime 路径，不证明正式组件树的原生窗口或三端 UI 验收；[验证范围收据](rust-agent-ui-bindings-20261019.json)。上游 main 的后续 PR #24 仅涉及 CI/测试/Gallery，不提供 #22/#23 所需公共运行时 API；当前 pin 保持不变。

## 完成账本

最新 [子代理 pane 接入记录](rust-agent-ui-child-20261019.md) 覆盖 typed 子源、直接 parent、独立 Composer/Indicators、有界只读加载与分页诊断、复用与撤销、单次读取及过期结果处理。本地 UI 增至 83 tests；原生 fixture 覆盖两条路径，但不是正式子 Transcript，也不是三端子 pane 的完整原生交互验收。

最新 [任务 pane 接入记录](rust-agent-ui-task-20261019.md) 覆盖 TaskPanelState 绑定、源指针替换、异步发送撤销、独立 Composer/Indicators 与原生 fixture 验证。本地 UI 增至 79 tests。显式宿主任务源与既有 Summary OpenTask 动作可接入，同会话 todo 的无独立 run 导航语义保持不变；仍不代表任务正式 Transcript 或子代理 pane 完成。

最新 [SideChat 接入记录](rust-agent-ui-sidechat-20261019.md) 与 [机器收据](rust-agent-ui-sidechat-20261019.json) 覆盖共享 pointer 绑定、独立 Composer/Indicators、创建/发送/取消关闭、Unknown 保留与父草稿隔离。本地 root 104、UI 74、工具 18 tests 通过，三端协议与组件到 GPU 路径复验通过；正式 Transcript 与完整原生交互仍不在这些通过项内。

最新 [Sidebar 操作接入记录](rust-agent-ui-thread-actions-20261019.md) 覆盖 ThreadRename、分叉导航、归档空选择与子代理 owner 归档刷新。基线 v0.3.8；root 105、UI 88、工具 18 tests 和局部原生回归通过，正式 Transcript/RunView 仍未完成。

| 要求 | 当前状态 | 还需要的证据 |
|---|---|---|
| Composer 独立输入/草稿/发送/取消及入口 | 基础组件、模型/新会话闭环、内容卡片/原子移除/批注评论、Main/任务/独立子代理/SideChat 附件选择/上传状态；局部原生与三端离屏证据分开记录 | 真实三端 pane 附件联调、预览/失败对账、选区捕获/来源高亮/内容 undo、其余设置入口、完整弹层/三端原生验收 |
| Indicators | 首批事实组件与 unknown/0/partial/error 测试 | 原生 RunView 呈现、主题/语义验收 |
| Transcript / RunView | 内容树与实验 RunView 已实现并接入公共 consumer | 其余 footnote、#22/#25、阅读/分页/流式关联与完整原生验收 |
| Sidebar | 共享服务绑定、选择/置顶/复制；重命名/分叉/归档已接入，局部原生 fixture 验证 | 真实三端操作与完整宿主验收 |
| SidePanel / Summary | Summary 原生挂载；任务、SideChat、独立子代理控制与绑定已接入；有界子历史只读加载诊断/分页已验证 | 各类 pane 正式 Transcript、真实三端子 pane 原生操作与完整联调 |
| 第二公开 API consumer | Composer、Indicators、Sidebar、SidePanel 公共组合，Summary 独立挂载；不依赖 RunSurface | 正式 RunView/Transcript 与完整原生宿主组合验证 |
| 原生窗口验收 | Composer/Indicators、Sidebar、Summary + 共享绑定的局部窗口验证已通过（合成文本服务） | 物理中文输入法、正式 RunView/Transcript/其余 SidePanel 内容和阅读/分页锚点验收 |
| review / CI / merge / tag | 草稿 PR #10 已提交；先前提交 CI 通过，后续增量仍需各自验证 | 完整要求审计、独立 review、最新 CI、合并与新阶段 tag |

既有三端 test-harness 到 GPU 记录继续有效，但它不是正式业务组件树的原生窗口验收，也不是以上全部组件已完成的证据。目标保持 active，不因初步测试通过提前收口。

最新的 [Composer/Indicators 原生接入记录](rust-agent-ui-native-controls-20261019.md) 进一步验证了实际公共组件与绑定的原生窗口和三端数据到 GPU 路径。该证据与原 RunSurface 测试路径分开，不提升尚未实现的 Transcript/RunView 或物理 IME 验收状态。

首批本地 gate（2026-10-19）：主 workspace 103 tests、UI workspace 42 tests（其中新增 4 个正式 Composer controller 用例、1 个 Indicators 用例、1 个第二 consumer 挂载用例），root/UI strict all-targets Clippy、fmt 与依赖边界通过。旧测试宿主在共享 delegate 提取后仍可构建并生成 GPU PNG；该图不证明正式组件原生窗口已验收。当前代码仍未提交/发布，TekesUI 的固定 188 输入重查一致。

本轮 SidePanel/Sidebar/Summary gate（2026-10-19）：UI workspace 56 tests（新增 Sidebar 4、SidePanel 5、Summary 3、独立 consumer 2），主 workspace 103 tests、Python 工具 18 tests；root/UI strict all-targets Clippy、fmt 与边界检查通过。新的 macOS UI consumer CI job 已声明，会构建 demo、运行组件/第二 consumer 测试，并校验 Cargo 实际解析的 8 个 UI package / 188 个文件。工作流尚未提交触发，不能称远程 CI 已绿。实际解析输入检查还拒绝错误 Git 来源、路径逃逸、变动/新增文件和 symlink。机器证据见 [本轮收据](rust-agent-ui-panels-20261019.json)。

最新 [附件宿主接入记录](rust-agent-ui-upload-host-20261020.md) 补齐 Main 的真实 macOS 选择器、受限文件读取、公共 ComposerUploads、原来源 pending gates 与窗口 wake；保留 callback/request/destination 校验和终态缓冲。真实选择器经独立本地 SEP fixture 走完选择、A/B 草稿恢复、发送/流式结算与取消，附最终 binary/AX/screenshot 证据；这不提升真实三端附件、物理 IME 或正式 Transcript 的验收状态。

最新 [附件三端验收](rust-agent-ui-upload-three-20261020.md) 通过原 Composer 入口和绑定分别验证正文＋附件、纯附件发送、单条持久化输入和重连无重复；Native/Pi 公开字节回读与会话隔离通过。Pi 历史保留路径引用，Native/DSH 保留文件元数据；DSH 通用文件下载不支持，不借用图片接口或读取私有文件。此为数据/离屏证据，真实系统选择器证据仍单列，正式 Transcript 与阅读/分页未完成。

最新 [SidePanel 附件记录](rust-agent-ui-pane-uploads-20261020.md) 接入任务、独立子代理与 SideChat 的同一选择/上传/状态路径，按原 binding/request 保留状态，区别同地址的独立 authority；关闭/换源后显示单独保留区，不回落父草稿。7 项新增 fixture 用例及真实本地 fixture 原生窗口证明分层记录；有界历史与独立单次子代理不提供上传。三端 Main 数据路径已复验，不冒充三端 pane 全部验收；正式 Transcript/RunView、预览/对账等完整目标仍未完成。

最新 [文件预览接入记录](rust-agent-ui-file-preview-20261020.md) 增加原会话公开读取绑定、逐文件 Composer 入口和宿主系统预览适配；Native/Pi 字节回读与 DSH 禁用已实测。原生宿主入口/读取/查看器进程启动已验证，Quick Look 内容视觉验收仍未完成；历史附件预览仍依赖正式 Transcript。

最新 [推理强度接入记录](rust-agent-ui-reasoning-20261020.md) 复用模型目录、ComposerChoices 与 settings/Send 路径，补齐公开档位选择、未知/缺能力及发送前验证。158 项 UI tests、第二 consumer、Native/Pi 档位配置回执和本地 fixture 原生交互通过；DSH 按本轮目录保持无档位入口，完整原生/物理 IME 及正式 Transcript/RunView 仍未完成。

真实中文 IME 验收步骤、证据要求及本轮未进入可确认组字的自动化尝试见 [原生 IME 验收](rust-agent-ui-ime-acceptance.md)。当前提交的 5 项原生窗口回归通过，但使用注入文本服务；不提升为真实输入法、正式 Transcript 或阅读锚点通过。

等待交互审查发现并修正 DSH 共享 actionable 回答信封适配，见 [SEP 对齐记录](session-endpoints.md#dsh-actionable-回答信封对齐2026-10-20)。真实 DSH 问题→回答→tool result→settle 已通过；Indicators 当前仍只显示等待事实，公开 actionable 业务组件及其原生 UI 仍待实现，不能据此认为该交互已交付。

最新 [首批 actionable 接入](rust-agent-ui-actionables-20261020.md) 增加共享审批/单选问题的公共卡片、原来源一次响应绑定和宿主回调；原生 fixture 与 9 项新测试验证回应、失效、重复点击、组字和离屏晚失败。复杂表单、自定义/secret 输入、显式对账及三端各自的 actionable UI 仍未完成，不提升正式 RunView/Transcript 的交付状态。

三端公开问答卡片的 Composer → 卡片 → 原 scope 回传 → 后续发送 → 新 epoch 历史恢复已完成局部联调，见 [三端问答记录](rust-agent-ui-actionables-live-20261020.md)。该证据不扩展为正式 Transcript、物理 IME 或真实审批验收。

Transcript 现已保留公开来源位置并提供 [事件顺序输入](rust-agent-ui-transcript-order-20261020.md)：已知事件与未定位预览分开，结果不反推调用位置。它不是完整块级内容布局；正式 Transcript/RunView、阅读/分页与原生验收仍未完成。

独立公开 [TranscriptPaging 控件](rust-agent-ui-paging-controls-20261020.md) 组合默认 TekesUI 按钮与状态文字，并复用原 binding 的一次性分页请求、原 scope 回执及 canonical has-more/stale。主会话和可写 pane 的测试宿主已接入，完整 Transcript/RunView 和阅读锚点仍待上游公开能力及原生验收。

文档的已落盘 native 消息改用公开 NativeEntry key，修复同时间戳历史前插时 timeline occurrence 改变造成的身份换绑，见[消息身份修正](rust-agent-ui-native-message-identity-20261020.md)。底层 Item ID 和流式行为保持兼容；无 entry 的临时流不能冒充持久阅读身份。可靠的流/记录阅读目标迁移仍属于 AgentKit 后续工作，并非仅等待 TekesUI 即可完成。

TekesUI #23 后继公开 8cf0840 已独立通过注册/API、实际八包输入闭包、隔离 AgentKit workspace 与有界原生选择/复制/追加/父滚动验证，见[CI 后继消费评估](tekesui-issue23-ci-review-20261020.md)。现已接入实验 UI workspace，原 431 基线及历史验收收据保留；没有将 #23 的局部技术接收扩展成 #22、物理 IME 或完整 Transcript/RunView 完成。

Native [消息生命周期边界修正](rust-agent-ui-native-stream-boundary-20261020.md) 防止同时间戳的新预览覆盖持久消息，并在刷新快照时原子保留 Pi 的 message_end 原因；消息结束不等同于 Agent/turn 结算。它不生成流到持久记录的阅读别名，完整 UI/阅读验收继续独立保留。

新增 [规范化内容块](rust-agent-ui-transcript-content-20261020.md) 保留公开消息内的正文/推理/工具/附件/未知块及覆盖状态，工具只引用既有 DTO。正式组件尚未实现；该模型不替代 TekesUI 文本、混合内容锚点或真实排版验收。

已补 [分页读取回执](rust-agent-ui-history-paging-20261020.md)：原 binding/epoch 的 Loading、Failed、Interrupted 与显式重试，复用现有 runtime / ChildRun 页合并。正式分页控件、文档安装与阅读锚点事务仍未完成。

首个实际内容行 [`InputMessage`](rust-agent-ui-input-message-20261020.md) 只组合 TekesUI 默认 Surface、只读正文及 caption，用户正文气泡和外置 footnote 右对齐；浅深色、短输入收窄、长中文重排使用库的公开文字服务验证。最初纯文本验收收据保持原范围；当前已增加附件/原始信息详情，见下方内容树接入记录，仍不是完整 Transcript / RunView。

[`OutputMessage`](rust-agent-ui-output-message-20261020.md) 现可独立挂载机器正文、推理与工具引用的局部展开层；按 canonical blocks 顺序消费，同一个正文控件在流式更新时保留选区。附件/未知块以明确的原始数据详情保留，Markdown 当前仍是源码文字；这是 Transcript 内容层进展，完整 Transcript / RunView、turn footnote 与混合阅读仍未完成。

当前 [`TurnFootnote`](rust-agent-ui-turn-footnote-20261020.md) 已接入 turn 级状态、provider 用量、毫秒时间戳的区间换算与共享完成回合 Fork。动作目录移入同一 presentation，Web 与 RunSnapshot 共用；原来源 ticket 在派发前再检查 pointer、epoch、边界和能力，已知完成可再次显式操作，Unknown 不自动重试。三端真实分叉/历史恢复、原生 footnote 点击和完整 fixture 回归通过；这不等于已交付其余 footnote 能力或完整 Transcript。

本轮还修正了原生测试宿主在 SideChat 撤销中重复读取快照的竞态：同一次 Composer/Indicators/actionable/paging 刷新消费同一 RunSnapshot，page receipt 必须匹配其 epoch。撤销后保留的旧快照只可读，不能因此恢复动作。新增确定性测试和此前失败的完整原生场景均已通过。


当前 [`TranscriptContent`](rust-agent-ui-transcript-tree-20261020.md) 将 InputMessage、OutputMessage、独立 ToolActivity 和 TurnFootnote 组合成公开内容树。源事件决定顺序，未定位内容明确单列；用户附件 → 正文气泡 → 外置 caption 均靠右，机器内容及 aside 靠左。纯附件不挂空正文文本目标。NativeEntry 和显式 submission receipt 保留既有控件；重复认领同一 receipt 会在修改前拒绝。实验宿主仅套用 TekesUI 的 ScrollContent；没有自行实现阅读状态或滚动补偿，完整 Transcript / RunView、#22 混合锚点与 #25 富内容仍待完成。


[ReadOnlyTranscript](rust-agent-ui-readonly-child-20261020.md) 已将 Native 有界子历史接入实际内容树、只读 source-call 详情和显式刷新/分页，不再仅显示数量。原绑定在 await 前分配 publication 并校验 epoch/存活；旧结果和旧页意图不能覆盖新 frame/获得新连接权限。读取回调一次性 admission，仍不构造可写 Run/Composer。组件、第二 consumer、原生多页消息渲染和真实 Native child 到 GPU 分层验证；混合阅读锚点/富内容/物理 IME 与完整 Run/Transcript 仍未完成。

[实际回合轨道组件](rust-agent-ui-turn-rail-controls-20261021.md)已接入原 RunView 的事件捕获、独立库滚动和生命周期隔离；默认公开 pin 明确 unsupported，#29 隔离候选验收与 #32/#33 缺口分列，尚不构成完整轨道或稳定 UI 交付。

[#32 普通回合 Start 的独立组件验证](rust-agent-ui-turn-start-20261021.md)已在新588277候选通过真实clip与高输入行检查；新身份的原生普通回合、宿主桥接与最终体验仍待验收，未替换默认公开pin。
