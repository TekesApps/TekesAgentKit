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
