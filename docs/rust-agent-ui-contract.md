# Rust Agent UI 组件接入契约

[Summary 原Content插槽](rust-agent-ui-summary-custom-20261022.md)已提供宿主产品行的原控件保留、动态测量与事件来源校验；安装/退役走原SDK生命周期，保留旧Run适配。完整repository具体行及未选中会话宿主布局仍需继续。

[Summary 工作区上下文](rust-agent-ui-summary-context-20261022.md)已补显式owner，无需虚构RunScope即可独立挂载host sections；原Run输入适配、owner隔离/旧动作拒绝、组件及原生兼容验收通过。通用host Content插槽见文首；完整repository具体行和未选中会话宿主布局仍开放。

[工具/推理与重排阅读](rust-agent-ui-disclosure-reflow-20261022.md)补齐视口上方已展开工具增长的组件回归，以及原生工具/推理点击、live resize和字号变化时原锚点/focus/中文marked保持。显式AX focus导航与被动重排分开验收；未提升为真实三端工具流或完整视觉通过。

[Composer notice](rust-agent-ui-composer-notice-20261022.md)已补输入卡片外提示、显式刷新事件及原控件保留；连接epoch/会话/显示版本隔离、原生组字内刷新与分页锚点回归通过。默认库文字按钮不冒充ClientKit原refresh glyph，完整Composer palette/chips/toolbar仍需继续。

[回合预览事实与 hover 卡片边界](rust-agent-ui-turn-preview-20261022.md)已补原turn/rail来源、240字素输出与组字期间只读校验；当前没有hover卡片，原Content被动浮层需求跟踪TekesUI #46，不以普通tooltip替代。

[当前固定程序的真实三端原生验收](rust-agent-ui-three-real-20261022.md)通过Kernel0.9.3、Pi1.3.0、DSH0.2.1-rc.2的分页/流式阅读/中文marked/取消恢复，并补精确程序与握手版本校验。Native完整生命周期仍有archive的session-running失败，未据独立UI模式提升为全量通过。

本文件是正式业务组件的实现契约与进度入口，不是完成声明。完整交付目标保持 Composer、Transcript、Indicators、RunView、SidePanel、Sidebar，加 Native/Pi/DSH 能力验证、第二公开 consumer、原生窗口验收、review/CI/合并与阶段 tag。

[最新公开 SDK 接收](tekesui-toolbar-public-intake-20261021.md)记录 09b4bab 的精确依赖、公开闭包、原生阅读/系统拼音证据及中断记录；此前版本的验收保持各自范围。

[Sidebar 原选项菜单](rust-agent-ui-sidebar-options-20261022.md)已消费当前公开 MenuContent：排序/分组的稳定事件、勾选/禁用、原偏好 CAS、重复排序的分页重置、第二 consumer 与原生中文组字屏障通过。Toolbar 的事件/逐项语义缺口已反馈 #36/#5；其余完整布局和阅读验收仍开放。

[共享会话菜单与置顶移动](rust-agent-ui-sidebar-session-menu-20261022.md)已成为 Sidebar 默认展示：原共享动作、原 CAS 移动计划、pending/旧来源拒绝、第二 consumer及原生组字/A-B草稿/换序通过；原生 rename/fork/archive也已走原菜单。未选中行的右键/键盘上下文入口跟踪 TekesUI #44，完整 Sidebar 尚未验收。

[公开版本回合导航](rust-agent-ui-published-navigation-20261021.md)已接通原生 demo 的普通 Start、Latest 和独立 rail 自动 reveal；复用原阅读 token、编辑器及宿主历史回调，中文合成预编辑专项通过。完整 rail/产品体验仍待验收，#22/#28/#32 的源码交付不再作为当前阻塞。
[回到最新按钮](rust-agent-ui-jump-latest-20261021.md)已补公开控件、原跟随命令及原生组字内点击验证，保留原编辑器且不记录轨道导航历史；完整selection/chrome仍待完成。
[有界只读子历史阅读](rust-agent-ui-readonly-reading-20261021.md)已接入原SDK事务，正文/状态/分页一起发布，公开consumer与原生fixture验证前插和刷新锚点；真实Native新pane验收及持久恢复仍单独开放。
[真实Native只读pane阅读](rust-agent-ui-native-bounded-reading-20261021.md)现有Kernel0.9.1原生锚点/刷新及元数据页耗尽证据；本case可见行5→5，多消息child前插仍开放。
[三真实harness的流式阅读](rust-agent-ui-stream-reading-native-20261021.md)已新增原生验收：流内追加时旧回合锚点、中文preedit与焦点保持，结算/重连/会话切换不重发、不串草稿；仍不等于完整UI或物理IME验收。
[Summary 用量两列布局](rust-agent-ui-summary-usage-20261021.md)将六项用量的标题与数值拆成原 TekesUI 文本槽，增加公开格式化回调；不重算 facts 或切换默认依赖。完整 Summary rows/slots 与原生产品验收仍待完成。

源码基线：AgentKit已发布v0.3.8 / `f1daa0ffcf6ced41e21f351e22845455ca78006c`；本分支Rust UI仍为实验0.0.0/publish=false。当前TekesUI精确公开pin为`09b4bab44b6dfa65487250ea816b610b9df59095` / `remediation-toolbar-checkpoint-20261021`，实际八包241输入，清单SHA `44b3174022584d3c099a45b86892d17dbf948905476c0f038be104df12aefe46`，以`ui/tekesui-source.json`和Cargo.lock核验。原431/8cf/2ee/129清单保留在`ui/baselines/`。

当前阅读进展见[正式事务](rust-agent-ui-continuous-reading-20261021.md)和[真实三端保留阅读](rust-agent-ui-retained-reading-20261021.md)：公开库API已消费，Native/Pi/DSH分页、A/B保留阅读及重连通过。销毁后新建组件树的持久恢复跟踪TekesUI #42；完整UI目标仍未完成。

阅读指南：下文保留了阶段性候选/默认输入和验收记录；其中历史的“未发布/等待交付/默认8cf”不覆盖本节当前pin，也不能继续作为当前阻塞理由。RunSurface始终只是协议/控件测试宿主，不是正式RunView。

Layout 和显示逻辑须逐项严格对齐当前 ClientKit 对应 Views；[对应关系与初步差异](clientkit-view-layout-review-20261020.md)列出源码及待验收项。组件类型存在、协议通过或 TekesUI 基础样式一致均不等于布局已经对齐。[严格共享列修正](rust-agent-ui-strict-column-20261021.md)已补源320/770规则、token推导净空和比例归一化；基础文档/Composer叠层现已推进，详见[underlay与阅读净空](rust-agent-ui-overlay-20261021.md)；完整rail、selection/Jump及其它清单仍未完成。[Composer显示规则](rust-agent-ui-composer-visibility-20261021.md)现已补归档隐藏、中断/暂不可提交保留、原实例与原生归档恢复专项；后续[降级快照修正](rust-agent-ui-degraded-frame-20261021.md)补齐坏历史期间的归档、epoch和目标隔离。

## 所有权

[Sidebar 首次恢复与观察失败](rust-agent-ui-sidebar-restoration-20261021.md)现已贯通 shared inventory 就绪事实、binding 和公开组件输入；未恢复不再显示确认空，观察失败保留最近列表。搜索/分组和完整 outline/status-pane 布局仍待继续。

[Summary 子代理状态来源](rust-agent-ui-summary-child-status-20261022.md)现由同 endpoint child 执行投影提供，旧 parent-only read 保留未知；状态附件仍开放。

[Summary 默认行文字带](rust-agent-ui-summary-row-layout-20261022.md)已补普通长标题换行、header/正文横向锚点与用量首行 metadata；原 SDK 控件、选择和 Composer 组字保留。静态行留白路由跟踪 #30，完整图标/普通 metadata/tone 及产品行仍开放。

[Summary 普通 metadata](rust-agent-ui-summary-metadata-20261022.md)进一步分离源标题与 trailing 槽：任务/child 状态不再拼入标题；宿主可捕获既有只读产品投影，格式化输出同 publication 冲突在改图前拒绝。plain metadata 已有首行布局、第二 consumer及原生组字验收；语义 runs/tone 跟踪 #25，状态附件仍为 #47。

[Summary repository divider](rust-agent-ui-summary-divider-20261022.md)已按过滤后的实际顺序组合原 SDK SeparatorView；仅非首个 repository header 显示，折叠/换序/owner 生命周期与原点击/组字验证通过。公开 layout 查询新增实际逻辑 divider band；hover/disclosure 指示仍需继续。

[Summary 焦点与组字](rust-agent-ui-summary-focus-20261022.md)对齐 header/普通动作行不接管 first responder，保留静态可选择文字和自定义控件焦点；demo 仅对独立 Composer 组字时的原可见 disclosure 放行。

[Summary 单行标题公开消费](rust-agent-ui-summary-label-20261022.md)接通当前精确公开 pin 的 `with_label_layout` / `shape_label`；宿主必须提供原 SDK label 服务。仅 header 单行尾部省略，完整源文本与 AX 名称保留，普通行继续换行。旧[候选记录](rust-agent-ui-summary-label-slot-20261021.md)按其历史输入解释。

[布局激活与成功帧边界](tekesui-layout-text-candidate-20261021.md)记录 #27 的独立候选消费。`render` 成功且原业务 publication/epoch 匹配后，宿主才发布当前原生文本、AX、几何和可见性；失败帧不靠更换编辑器或重试 Agent 操作恢复。

### 排版与基础样式

[Summary section 顺序与折叠](rust-agent-ui-summary-sections-20261021.md)现已补齐宿主优先顺序、按 section ID 保留的折叠、真实隐藏的输入/AX 子树及任务 pointer 改变时的旧动作拒绝。公开 DTO/查询支持第二 consumer；单行省略已消费当前公开 SDK；完整 Summary 默认行及整体视觉仍未验收，自定义 host Content 见后续接入文档。

[Composer 输入行与单一主动作](rust-agent-ui-composer-band-20261021.md)已按对应 View 的内容顺序推进：正文与主动作/Add 共用输入 band，发送/停止互斥并保留原控件，空 setup 不占位，十行上限随字体倍率更新。模型/预设的宿主 toolbar、完整 palette/chip 与 glyph slot 仍待对齐，notice 见文首当前记录；默认 TekesUI 样式和固定 pin 不变。

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

[三端流式原生专项](rust-agent-ui-stream-native-20261020.md)已验证正常 Send、真实预览、落盘后的原行/正文控件/选区、会话草稿和重连不重发，并修复聚合/结构化回补时的唯一正文/推理控件迁移；多块内容不猜测关联。这是早期专项的原范围；新增流式阅读锚点/合成preedit证据见文首链接，实体输入法/候选窗与完整交互仍须分别验收。

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

- [TekesUI #22](https://github.com/TekesApps/TekesUI/issues/22) 的混合Content稳定item与更新事务已在当前公开pin交付，并在正式Run和三端原生分页中消费；issue仍为open，不代表源码不可取得。不可将多块Transcript拼成单一文本框替代。
- [TekesUI #42](https://github.com/TekesApps/TekesUI/issues/42)：混合Content持久检查点的公开导出/新host恢复尚缺，不能复用旧attachment或在AgentKit手算像素补偿。
- [TekesUI #25](https://github.com/TekesApps/TekesUI/issues/25) 新增：可选中富内容 Component、受控流式更新与原生文本映射。现有 MarkdownView 提供有界 Markdown 被动绘制，不能据此宣称已有可选中富内容组合接口。当前源码文字显示和显式原始数据详情保留内容，富内容样式继续待接入。
- #23 的无表单只读 Content API 已由当前固定 8cf 后继实际消费。原61dd候选的注册失败保留在[历史评估](tekesui-issue23-public-review-20261020.md)，后继的通过证据见[独立复验](tekesui-issue23-ci-review-20261020.md)，不再列作当前纯文本阻塞。

基础能力由 TekesUI 实现；收到公开不可变版本后显式升级并验证，不跟随活跃 WIP。AgentKit 自身仍需完成消息组合、stream 到 durable 的可靠阅读身份关联、其余 footnote 动作、正式 Transcript/RunView 和 pane 消息接入，不把这些工作归为上游阻塞。

## RunView（实验组合已实现）

RunView 组合正式 Composer / Transcript / Indicators，拥有 Agent 组件间意图路由；不拥有窗口、其他产品导航或第二套 Run 状态机。异步通知继续订阅已有 presentation，通过公开 WindowWake 唤醒。

当前公开 [`RunView`](rust-agent-ui-run-view-20261020.md) 已组合 Composer、Indicators、分页、问答和可安装的混合内容树，提供同帧 update 与类型化意图出口。实验主会话和可写 pane 已复用它，第二 consumer 可独立构造；RunSurface / ComponentDemo 仍仅作测试宿主。完整 Transcript/RunView 验收尚未通过：可靠 stream-to-durable 关联、#22 阅读/分页锚点、#25 富内容及物理 IME/完整三端原生 UX 仍需完成。

## Sidebar（首批实现）

[搜索 footer](rust-agent-ui-sidebar-search-footer-20261021.md)现已接入独立显示输入、底部锚定、长错误下 Retry 可见性及捕获原请求的公开回调。[共享单端查询控制器](rust-agent-ui-sidebar-query-controller-20261021.md)现已接入 presentation 与公开 RPC；[未知命中的公开库存恢复](rust-agent-ui-sidebar-inventory-refresh-20261021.md)已补齐；[完整provider结果成员投影](rust-agent-ui-sidebar-search-results-20261021.md)已接通原publication、身份联结、未知库存恢复、查询隔离摘要元数据及清空恢复；跨端聚合、本地FTS OR/BM25、排序偏好和完整outline仍待完成，不由宿主另写业务状态机。

`Sidebar::new(namespace, labels)` 实现公共 Component，也可 `into_host()` 独立挂载。每个组件使用独立的 u32 ID namespace；输入 `SidebarInput` 提供单调 inventory revision、loaded/error、按消费方分组/排序偏好排列的 rows 和当前 SessionAddress。首次恢复前不显示确认空；观察失败保留已恢复列表。选择来自独立宿主导航，可在同一库存 revision 下变化。

每行直接提供共享服务的 ThreadActivity、ThreadPinPresentation 与 ThreadActionsPresentation。组件检查动作的 endpoint/session 与行身份一致，不从标题、编码 ID、是否有运行来猜权限/完成状态；置顶关系不决定显示顺序、不写 UI 自有存储。选择回调携带完整 SessionAddress，同名 session 在不同 endpoint 中保持独立。

选中行使用 TekesUI ButtonContent 已有的 selected 属性、quiet 样式及语义。当前把所选会话的共享动作显示为侧栏操作区；按共享顺序显示标签与 disabled reason，不新写菜单/按钮状态机。完整产品的菜单外观、键盘和视觉仍待原生验收。

`take_intents` 只产生 `Select` 或 `Action {address, definition}`。copy/rename 保留原始 interaction/copyText；执行经已有 SharedAgent 的 thread.performAction，使用同一个 RPC identity 显式对账，不能自动重试。UI 回调不是执行许可：共享 dispatcher 必须按当时的新鲜能力重新核对。普通状态更新/排序保留原按钮实例；移除后再加入的项目不复用已退休的 UI action ID。

验证已包含真实 SharedAgent + 独立 SEP fixture + ThreadPreferences：一次 pin、同 RPC 重放、再次读取共享状态与重连均通过。该用例不代表三端原生窗口已验收。

## SidePanel 与 Summary（首批实现）

[Summary 静态行可选中文字](rust-agent-ui-summary-selectable-20261022.md)已消费原 SDK 只读内容组件：普通静态行和用量标题保留原 owner/选区，章节隐藏与作用域淘汰继续由原组件处理；公开 `passive_text_id`，不新增剪贴板状态机。完整默认行与自定义host内容仍未完成。

[正文视口与顶部净空](rust-agent-ui-sidepanel-layout-20261022.md)公开 `SidePanelLayout` 与最近 arrange 几何，普通 Agent tab 采用 regular inset，短内容不收缩 pane；圆角子树裁切/focus 边界待 TekesUI #45，inline demo tabs 不算窗口 toolbar。

[父投影与 pane 生命周期](rust-agent-ui-sidepanel-projection-20261022.md)明确等待时不构造正文、跨 thread 销毁 UI、同 thread 保留 tab、草稿与 pointer 不随 UI 销毁；最终程序的原生分页/reveal/组字证据也在该记录中。

`SidePanel` 是 Agent 业务 tab/pane 容器，不再包一层 RunSurface。输入 `SidePanelInput` 提供父 RunScope、单调 revision、打开的 SidePanelTab、当前选择与现有 SideChatSnapshot。新增必填 `projection_ready` 表示父投影是否存在，不能从连接新鲜度或子历史加载状态推断；已知父投影的 `SideChatBinding::input` 默认 true，宿主可明确覆盖。第二 consumer 提供 `sync_scratch_tabs_with_projection`。task tab 来自 AgentTask；subagent tab 必须显式提供 ChildHistoryLayout；SideChat tab 来自实际 SideChat。`supports_side_chat` 使用现有 controller 的能力结果：Some(true)/Some(false)/None 分别为支持/不支持/尚无结论，不将未知显示成不支持。

选择回调携带原始 parent 和完整 tab 定义，包括子历史策略。关闭与创建只发作用域明确的意图，不自行 fork/discard/cancel。SideChat unknown/closing 时禁用再次关闭，创建 pending/unknown 时禁用再次创建并呈现共享错误；在共享 controller 仍持有同一临时 pointer 时，组件拒绝从输入中移除该 tab。确认 discard 后才能移除，不能用隐藏 UI 冒充关闭成功。

pane 内容用独立 namespace 的 Kit Component 组合：

1. `update` 打开/选择 tab 后获取 `content_token(kind)`。
2. 数据首先通过现有 TaskPanelState 的 commit/generation 或 ChildRun / SideChatController 校验；subagent 仍用明确的 bounded/independent adapter，不能回退为父历史。
3. 将实际 Kit 内容包装为公开 `Content`，通过 `install_content(token, content)` 放入对应 pane。上游正式 Transcript 尚未可消费时，未挂载位置只是明确的待接入位置，不能算子运行内容完成。
4. 同一 endpoint/thread 内，普通更新及 tab 切换保留精确实例，父 scope 缓存有明确上限。跨 endpoint/thread 时释放旧 UI；父投影等待时清空正文与缓存，不显示伪空态或创建 pane。草稿、运行与 pointer 仍归原 binding/controller，不取消运行或偷偷 discard。销毁重建后的持久阅读恢复仍跟踪 TekesUI #42。
5. PanelContentToken 只约束 UI mount 生命周期，不能代替业务 load token。A→B→A、ready→waiting→ready、关闭重开、history layout 改变、父缓存淘汰会使旧 token 失效。等待时 `content_token` 返回 None，Create/SideChat close 禁用，tab 仅保留导航元数据。相同 revision 的冲突输入被拒绝。

宿主在结构切换前排出旧动作，先完成 pane 内组字，再通过公共 ContentHost 解除被销毁子树的 focus；主编辑器可继续组字，不受 pane 生命周期干扰。异步子请求捕获父投影 generation；晚结果可保留有效业务 binding/pointer，但不能越过新 generation 接管选择。窗口级总体导航归宿主，pane 组件不另建业务状态机。tab strip 当前由基础 ButtonContent 组合；选中语义仍来自其现有 selected 属性，完整原生 tab 交互与视觉尚未验收。

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

| 要求 | 当前已验证范围 | 仍需完成 |
|---|---|---|
| Composer / Indicators | 原 RunView 中复用 presentation；草稿、发送/取消、入口与捕获来源、unknown/partial 状态；已有局部真实三端和原生窗口证据按各自程序解释 | 其余设置/附件 pane 组合、完整 Source 布局与交互/语义、物理输入及配对视觉 |
| Transcript / RunView | 正式混合 Content、原 SDK reading/reveal 事务；分页20→44、流式/工具增长/重排、A/B保留阅读与重连、Start/Latest、原焦点/组字；真实三端历史/流式证据保留原程序归属 | #25富内容、#42 fresh-host持久阅读、#46结构化hover、完整footnote/rail显示规则和配对视觉；剩余真实多消息有界child组合 |
| Sidebar | 原 inventory/search/folder/pin服务、共享操作、选中行补入与自动reveal；部分原生fixture与重挂载/过期请求fencing | #44原行上下文触发、完整列表阅读生命周期、剩余真实三端/目录/活动组合及整体布局验收 |
| SidePanel / Summary | 共享pane状态与原RunView/只读内容、投影ready与生命周期、host clearance、Run/Workspace Summary、可选择文字、用量列、自定义host Content、header单行/焦点/repository分隔规则；child状态来源、源标题带/普通metadata/用量首行对齐及格式化publication校验及显式 owner/层级缩进已补 | #36/#5 toolbar、#45语义Panel、#47图标/不定状态附件、#30只读留白路由、#25语义runs、#38展开指示；默认行 leading/tone、具体产品行、完整tab/chrome和配对视觉 |
| 第二公开 API consumer | 当前UI workspace含独立consumer；公开inputs/queries/actions、原SDK text/reveal及host适配、child状态更新保留原pointer | 完整目标的组合体验与宿主验收，不能以编译/局部组件测试代替 |
| 原生 / 三 harness | 当前窗口验收及系统拼音软件输入；真实Native/Pi/DSH协议/阅读/流式保留各自program scope | 新child状态在真实三端的验收、Native0.9.3 lifecycle/archive已知未过项、实体键盘/候选窗/VoiceOver与整体体验 |
| review / CI / merge / tag | PR #10仍为实验草稿，增量提交分别检查CI；本地验证和最终程序收据见各阶段文档 | 完整要求逐项审计、review、最终CI、合并和新阶段tag；整体目标尚未完成 |

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

[实际回合轨道组件](rust-agent-ui-turn-rail-controls-20261021.md)已接入原 RunView 的事件捕获、独立库滚动和生命周期隔离；当时8cf默认输入明确unsupported；当前1294的基础reading/reveal/Start已公开，#29隔离候选记录与#33需求仍需按实际消费范围核对，尚不构成完整轨道或稳定 UI 交付。

[#32 普通回合 Start 的独立组件验证](rust-agent-ui-turn-start-20261021.md)已在新588277候选通过真实clip与高输入行检查；当前1294已包含Start；当前正式桥接及验收见文首公开版本记录；整体体验仍待完成。

后续[主导航宿主回调与588277原生Start](rust-agent-ui-turn-navigation-host-20261021.md)已增加显式显示事实、原点击历史捕获和原生组字内选区检查；此前收据保持原范围，完整交付仍未完成。

最新 [Sidebar 标题与活动映射](rust-agent-ui-sidebar-inventory-20261021.md) 已接通 reconciled projection 标题、显式清空和单绑定内最近活动/运行冻结，默认 UI357与共享服务9项回归、独立 consumer、原生 fixture 及三端离屏复验通过。活动排序尚未跨进程持久化，标题排序偏好、置顶区/顺序、文件夹、跨端与本地FTS仍待对齐；未提升默认SDK或完整UI交付状态。

最新 [Sidebar置顶区与偏好](rust-agent-ui-sidebar-preferences-20261021.md) 已接通独立可折叠置顶区、原行保留、公开标题排序/置顶顺序输入和带持久revision的同一偏好存储；无比较器拒绝标题排序，宿主提供系统本地化比较。默认UI362、无UI183与独立consumer37项、实际原生折叠通过。原生选项/移动菜单仍待TekesUI #36，文件夹、多端、本地FTS、持久活动时钟和完整Views/最终验收仍未完成；未提升SDK或发布新的headless版本。

后续 [Sidebar查询帮助与目录](rust-agent-ui-sidebar-help-20261021.md) 已将当前查询摘要和行成员关系放进同一公开输入，缺少摘要时回退原会话目录。默认公开pin仍只提供元数据，`sidebar_row_help_supported()`为false；隔离的固定336候选在原Content上消费SDK提示声明。默认UI364、候选UI391（含原26项候选回归和wrapper检查）、独立consumer38项与严格Clippy通过；实际窗口查询更新/目录回退/关闭保持/合成组字期间原点击门禁通过，三端公开查询与内容离屏回传复验通过。原生关闭经`cancelOperation:`，无物理Escape/IME或VoiceOver验收；SDK空字符合成控制键缺口为#37。来源、失败保留、像素检查和清理收据见同名JSON。完整Views、阅读锚点与最终交付仍未完成，PR保持草稿。

再对照 `ClientSessionSidebar.sessionRows` 后修正[搜索与置顶区](rust-agent-ui-sidebar-pinned-search-20261021.md)：搜索只过滤普通会话，独立全局置顶行在 Searching/失败/无命中时保留原控件。`loaded`只代表库存恢复，新增`SidebarSearchHelp.results_ready`区分结果等待，未知hit不伪报无命中。默认367/固定336候选394项、第二consumer39项、严格Clippy、原生置顶搜索和三端离屏回传通过。默认精确pin、headless契约及其余未完成范围不变；来源与边界见同名JSON。

后续 [Primary Folder 分组](rust-agent-ui-sidebar-folders-20261021.md) 已接通端点隔离的 ID/路径/No Folder 身份、排序后首次出现的组序、持久分组/折叠及独立于全局 pin 的筛选。原行控件在分组/取消分组/折叠时保留，筛选退休旧叶子动作；搜索投影保留显式 filter。偏好文件升级为4并迁移1/2/3，旧二进制不能读取4，尚未发布。默认UI372、固定候选399、第二consumer40、无UI186项及严格Clippy、三端离屏回传、实际原生分组/折叠/筛选通过。标准 disclosure 指示器与触发器焦点连续性待SDK #38；选项菜单待#36，完整管理目录/按文件夹新建、分页/选中补齐、多端与其余完整UI目标仍未完成。

最新 [Sidebar选中导航](rust-agent-ui-sidebar-navigation-20261021.md) 将显式导航展开与持久折叠分离，复用宿主原导航版本；普通刷新、重复/过时请求和晚到的目标行不能覆盖后续手动折叠。无变化偏好确认、失败回滚和筛选/置顶边界已接通；不做隐式补读或滚动。默认UI380、固定336候选407项、第二consumer41项及严格Clippy、原生导航/折叠/标题刷新和三端离屏回传通过。无UI核心与格式4未改，沿用上一阶段186项；缺失选中行的独立补读/分页、滚动可见性、基础组件及其余完整目标继续保留。

Sidebar local inventory now pages by five with independent global pins and a selected-row supplement; search is not re-paged. Versioned visibility captures consume the fixed candidate original item reveal without moving focus. The published default pin remains unchanged; see [paging and visibility contract](rust-agent-ui-sidebar-paging-20261021.md) for capability and native-host acceptance limits.

The Sidebar selected-row adapter now reads one canonical memory item independently of the displayed inventory page, with source/epoch/selection fencing and no provider call or history hydration. Navigation validity remains separate from filter/search/pin membership. See [selected-read contract](rust-agent-ui-sidebar-selected-read-20261021.md); unknown canonical sessions are not fabricated and original TekesUI reading/reveal publication gates still apply.

[Full Sidebar folder catalog](rust-agent-ui-sidebar-catalog-20261021.md) now derives occupied folder choices from canonical full-inventory effective activity, before filter/search/sort/paging. Public SidebarPublication publishers supply `activity_at` alongside rows; the demo and independent consumer read the versioned catalog. This does not add empty-folder management, standard menus or default native reveal, and the persistent activity-clock gap remains.

The [published reading checkpoint integration](rust-agent-ui-published-reading-20261021.md) supersedes the earlier lack of publicly obtainable #22/#28 source. The exact default pin now exposes original reading/item reveal; a source-fenced public adapter and explicit native Sidebar navigation use it. Navigation-only attachments do not establish continuous Transcript/inventory reading transactions or page-anchor preservation; those remain separate required work.

The subsequent [Run continuous reading integration](rust-agent-ui-continuous-reading-20261021.md) connects the existing whole-frame plan to original SDK transactions in the formal host. It removes the duplicate Transcript write, uses independent display revisions for gate-only refreshes, and validates native owned-fixture pagination with marked text and focus retained. This does not accept all three harnesses' history anchors or Sidebar's full inventory reading lifecycle.

The [retained-reading follow-up](rust-agent-ui-retained-reading-20261021.md) fixes stale completed-turn catalog removal and verifies native public pages, A/B retained reading and reconnect on Native/Pi/DSH. It distinguishes button focus from publication focus, and editing-authority tokens from draft state. Fresh-host persistent checkpoints require TekesUI #42; bounded read-only panes, full Sidebar reading and dedicated native streaming-while-reading checks remain open.

[Footnote display-time integration](rust-agent-ui-turn-footnote-time-20261022.md)
adds explicit host-clock revisions and calendar callbacks to the existing Run/Transcript
reading transaction. Live elapsed/positive tokens and settled source end timestamps
replace diagnostic status prose; stale clocks, interrupted/noncontinuable owners and
old plans are fenced. Shared components own display rules; system time/calendar and
wake scheduling remain host adapters. This does not complete footnote layout parity.

[输出脚注显隐](rust-agent-ui-output-aside-20261022.md)进一步对齐 Source 的 live/output/settled-fork 门槛；结束时间、纯推理或工具本身不生成输出脚注。输出存在不授予复制、反馈或执行权限，原信息节点在显隐之间保留。

[反馈脚注与辅助焦点](rust-agent-ui-feedback-aside-20261022.md)将评价挂载纳入输出脚注的同一显隐门槛，保留原响应授权与组字 preflight；备注仅在已有评分时显示，并展示 committed 文字。复制、回合及评价辅助按钮消费原 SDK 保焦点策略。单行整体排布仍未完成。

[Summary 层级排版](rust-agent-ui-summary-indent-20261022.md)新增显式只读 indent_level 回调：按 Source 层级移动标题和减少其换行宽度，metadata 保持右端，原动作/文本 owner 不变；格式化结果随 publication 捕获。完整单行 Transcript aside 的图标渲染服务缺口已补充到既有 #47，未复制 SDK 实现。
