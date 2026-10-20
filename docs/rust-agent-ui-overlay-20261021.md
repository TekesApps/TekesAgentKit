# RunView 文档 underlay 与 Composer 净空（实验）

本轮替换正式 RunView 的纵向 docked 文档/Composer 排版。对照 ClientKit `RunNativeController.configureTranscript` 的文档下层、`UserControlsOverlayController.viewDidLayout` 的底部 controls、`RunView.composerBottomPadding` / `composerContentSpacing` 和 `RunOverlayView.layout` 的零尺寸保护。只改 Rust AgentKit；不改 Swift、Kernel 或 TekesUI 输入。

## 当前实现

- ScrollContent 保留完整 pane frame 和独立滚动坐标；正文按原 RunColumnLayout 排版，与 Composer 共用横向边界。使用库的单轴滚动策略，不复制其 focus gutter、光学 inset 或滚动算法。
- Composer 是文档上方的 retained sibling，离底部一个 `Spacing::Regular`，高度来自原 Composer 的公开测量并受 pane 可用高度约束。阅读净空另外加 `Spacing::Loose`；它属于文档遮挡，不增加 Composer 自身高度。
- TranscriptContent 在库的 ContainerLayout 中保留文档净空；ScrollContent 收到真实可见 clip。只有库读写 scroll offset / reading attachment；AgentKit 不用 append/prepend 像素差补偿。
- 末行在库的 scroll limit 内能停在 Composer 净空之上。Composer 上的 wheel 不泄漏到历史；框架拥有编辑器内部滚动和 pointer/IME/focus。
- 空 ActionableCard 关闭语义子树，不再占 header gap；原 owner、操作回执和命名空间仍保留。
- 任一 pane 维度为0时，不测量/重排子树，保留原几何与 scroll，使用库 inactive 子树语义停止绘制、输入和 AX，恢复时不抢焦点。宿主每一帧都必须提供有效 TextSurfaceHost，包括重新激活的帧。

`RunView::layout_geometry()` 返回实际 arrange 的 pane、正文列、Composer、可见文档 clip 与上下净空，以及所对应 publication / epoch。它是只读布局事实，不是第二个 Run/reading 状态。`is_pane_presented()` 为 false 时几何是保留值；更新模型但尚未 arrange 时，也不能把新 snapshot 与旧几何配对。第二公开 consumer 的 `arranged_run_geometry` 展示这些检查，`composer_focus_target` 同时检查 pane 和 Composer 可见性。

## 证据范围

现有36组列/字号倍率/主题测试现在检查真实 underlay pane frame、消息行与 Composer 列边界，以及 Regular 底边和 Loose 阅读净空；25组冻结 ClientKit 横向向量仍检查 controls/column。新增折叠恢复和末行/滚轮路由测试保留真实原编辑器；未把旧 docked 断言悄悄保留为新验收。

隔离候选2b16的正式 Run 探针扩展至8项：原预检/ABA/组字/完整 frame 提交、追加/前插与 Composer 增高保锚仍通过；新增折叠后恢复原 reading attachment、anchor 和行内位置。探针 wheel 使用实际可见 clip 的中心。该源包463文件未修改，默认公开8cf pin没有提升；默认宿主的混合阅读事务集成仍待完成。

原生 context、归档故障恢复、常规组件回归和可选系统拼音软件探针分开记录，精确结果、源码/二进制/截图哈希见[JSON收据](rust-agent-ui-overlay-20261021.json)。现有截图 `/tmp/agentkit-overlay-native-context-20261021.png` 是本地 fixture，不是与当前 ClientKit 同内容、主题、窗口、倍率和正常字号的配对视觉证据。

## 仍未完成

状态、分页、actionable、宿主附件控件目前仍组合在顶部区域；完整 leading rail / chrome、selection 和 Jump、bounded-document-panel 的单独 viewport 配置、各 pane 与 Composer 内容布局仍需对齐对应 ClientKit Views。基础 underlay 不等于整个 RunOverlay/RunChrome 已交付；Sidebar/Summary/SidePanel 的视觉和显示逻辑审查继续。

实际三端阅读/分页、默认/native mixed reading、#25 富内容选择、实体 IME/候选窗、VoiceOver、完整等条件 ClientKit 视觉验收仍未通过。本轮不据此合并、打 tag 或宣称完成完整目标。

另有已复现的通用诊断缺口 [TekesUI #27](https://github.com/TekesApps/TekesUI/issues/27)：inactive→active 在 layout 中发生时，缺少文本服务的错误 renderer 能绕过 render 前置校验。通用 composition 最小探针位于 `scripts/probes/tekesui-active-layout-preflight.rs`，在同一只读2b16候选上独立复现；它是 known-boundary 测试，不能计为该能力修复通过。正常 AgentKit 宿主始终有文本服务；消费侧不复制库校验、焦点取消或更换编辑器来规避。上游需明确激活契约或补齐校验。
