# ClientKit Views → Rust Agent UI：布局与显示逻辑核对

这是接入契约和未完成项，不是完整视觉/行为一致性声明。参照当前 ClientKit 工作树中的对应 Views，不能仅因 Rust 类型已存在或协议测试通过就认为 UI 对齐。源码路径、HEAD 与实际文件哈希见[JSON](clientkit-view-layout-review-20261020.json)；ClientKit 有未提交 WIP，已按当前文件读取并保留，未改动。

## 对应关系与验收责任

| Rust AgentKit | ClientKit 参考（仓库内路径） | 需要核对的 Layout / 显示规则 |
| --- | --- | --- |
| Composer / ComposerContext | `Sources/Composer/ThreadComposerView.swift`、`ComposerControlsLayout.swift`、`ComposerCardView.swift`、`UserControlsView.swift` | 内容卡片、入口/菜单、正文与动作区域的顺序和可见性；空卡片不占位；草稿、组字和动作状态保持原目的地；窄窗/增长正文不挤掉发送和取消入口。 |
| TranscriptContent / InputMessage / OutputMessage | `Sources/TranscriptCoordinator/TranscriptNativeSurface.swift`、`ThreadTranscriptPresenter+Scroll.swift` | 历史/预览身份、机器左/用户右、展开推理/工具、正文与附件顺序、空/加载/错误、向上阅读、前插、切换与恢复。使用 TekesUI 的公开排版和阅读事务，不移植 Swift 渲染器。 |
| TurnFootnote / TurnFeedback | `Sources/TranscriptRendering/TranscriptTurnFootnote.swift`、`FeedbackNoteEditorView.swift` | 用户 aside 在圆角气泡外靠右；机器 footnote/评价靠左；只显示真实源允许的复制、用量、评价与 completed-turn 动作；历史错误保持发生位置。 |
| Indicators | `Sources/TranscriptRendering/TranscriptIndicatorRailView.swift`、`Sources/Composer/Agent/AppServerHealthView.swift` | 连接/执行/等待/失败/用量状态与回合位置导航分别核对；不能把目前单行状态文本等同于完整 indicator rail。 |
| RunView | `Sources/Composer/Run/RunView.swift`、`RunNativeController.swift`、`RunOverlay.swift`、`RunChrome.swift` | Transcript、输入区域、回到最新与辅助 chrome 的相对位置；共用会话列边界；原生树保留；bounded pane 与主 Run 区分；归档/不可写的显示规则。 |
| SidePanel | `Sources/ToolPanels/ClientSidePanelHostView.swift` | header/tabs/正文分层、切换 tab 保留子控件、关闭淘汰原 owner、目标投影未到不显示旧/空源内容；窗口级 toolbar 放置仍由宿主决定，不把系统标题栏移植到库。 |
| Task / Subagent / SideChat pane | `Sources/Composer/Agent/ClientAgentTaskView.swift`、`ClientEmbeddedRunView.swift`、`ClientSideChatRunView.swift` | task ID 与其动态 session pointer 区分；loading/missing/error 与内容区分；可写源复用 RunView、有界子历史复用只读 Transcript；独立草稿不串主会话。 |
| SummaryView | `Sources/Composer/Agent/ClientAgentSummaryView.swift` | section/row 顺序、折叠、空/不可用、原 item 动作；同 section/item 的控件保留；宿主附加 section 不覆盖基础事实。 |
| Sidebar | `Sources/Composer/Agent/ClientAgentSidebarListView.swift` | 恢复前 loading、恢复后 empty、选中项、活动/置顶与共享操作、搜索状态和重命名；不将恢复未完成显示成已确认空列表。 |

## 已确认的差异，仍需完成

1. Rust RunView 当前为纵向 Stack：Indicators、历史控件、actionable、Transcript viewport、Composer。ClientKit 的 `RunNativeController` + `RunOverlay` 保留文档与叠层，`RunChrome` 为 leading gutter 提供 indicator rail，并把 Composer/selection 放在其上。**当前 Rust 组合不等于该布局已完成**；后续应使用 TekesUI 公开组合/布局能力对齐相对层次、列边界与可见性，不能用 RunSurface 测试窗口代替正式 RunView。
2. 当前 Rust Indicators 已有连接/执行/等待/失败/用量事实与文本显示，但完整回合 rail 的位置映射、选中/hover/跳转及可达性仍要逐项核对。阅读位置基础依赖 #22；不能用业务侧像素估算填补。
3. Transcript-only 候选事务已验证，完整 RunView 中受控编辑器可能拒绝；缺少无副作用预检已提交 [TekesUI #26](https://github.com/TekesApps/TekesUI/issues/26)。不把 fallible 更新直接塞入现有阅读回调。
4. 当前进程 A/B 恢复不是 ClientKit 的持久 checkpoint 恢复；#25 富内容选择、真实三端阅读分页、实体 IME/候选窗、原生动量/VoiceOver、全部入口与最终并排验收仍未完成。
5. 本文逐项给出待核对规则，不将表中所有组件标记为已验收；完成需有正式组件树、第二公开 consumer 和原生窗口证据，并对应实际不可变依赖。

## 不随对齐而复制的内容

布局和显示逻辑参照 ClientKit；颜色、字体、圆角、语义间距、控件反馈使用 TekesUI defaults/tokens/主题。不复制 AppKit/Metal 的光学补偿、固定像素样式或通用焦点/IME/滚动实现。宿主拥有窗口、总体布局、导航与系统适配；共享库不得把 Agent 行为或控件实现退回宿主。只读源码对照不引入 Swift 源码或 Swift 构建步骤。

后续：[正式 RunView 共享会话列](rust-agent-ui-column-layout-20261020.md)已补居中限宽、全宽和非对称边距，并记录36组几何测试。上述初步差异中，共享水平边界已前进；叠层、完整 rail 和最终并排验收仍未完成。
