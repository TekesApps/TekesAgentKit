# 严格对齐 ClientKit：共享水平列

正式 RunView 按当前 ClientKit `ClientRunColumnLayout.geometry`、`UserControlsOverlayController.viewDidLayout` 和 `WorkspaceLayoutMetrics.conversationColumnGeometry` 修正列规则。冻结的语言中立参考数据在 `ui/demo/tests/fixtures/clientkit-run-column.json`，记录源文件哈希、PreviewDesignTheme token 基线及25组字面预期；未引入 Swift 文件或工具链，未修改 ClientKit、Tekes、Kernel、TekesUI。

- Reading：最小320、默认最大770；leading 为 `max(horizontal_inset, (viewport-width)/2)`，过窄 pane 保留最小列宽。最大宽度是 Agent 阅读布局约束。
- rail 宽度是 `ControlSize::FocusBar + 2 × Spacing::Loose`，horizontal_inset 再加一个 Loose。当前固定 TekesUI 为34/50；实现消费 token，不固定这两个结果。
- FullWidth：保留两侧 horizontal_inset，正文宽度下限0；比例 pane：各比例有限时归一化到0..0.45，非有限变0。省略 trailing 使用原 leading。`from_view_inputs` 保留源优先级：显式 leading 比例优先于 fullWidth；单独 trailing 不启用比例模式。
- Transcript、Composer 及 Run 控件使用同一水平边界。Composer 外框服从被分配的列，工具栏固有宽度不扩大外框。保留 TekesUI 布局和焦点绘制；没有自制布局引擎或裁切焦点外溢。
- `RunView::minimum_pane_width` 向宿主提供可用 Composer 的建议下限，窗口、侧栏、分栏折叠策略仍属宿主。demo 为同时展示两个 Run 预留两条 reading column，当前最小窗口1068点；这不是产品 SidePanel 的最小宽度或折叠策略。离屏输出尺寸与宿主保持一致。

颜色、字体、圆角、控件反馈及语义间距继续使用 TekesUI 默认组件和 token。升级精确固定的 UI 版本后统一体现其基础样式变化；不为复刻旧皮肤覆盖库样式。机器内容与 footnote 靠左；用户气泡与气泡外 footnote 靠右。该业务归属不随主题升级改变。

验证包含正式 Run 的36组主题/缩放/宽度组合、25组源预期几何/归一化、View 输入优先级和 demo 最小宽度，以及第二公开 API consumer。当前轮首次几何测试发现窄 FullWidth 下 Composer 固有工具栏宽度扩大外框，修正后保留相同断言重测通过；原失败日志保留。候选2b的7项受控更新/阅读事务与严格 Clippy也独立回放通过，默认公开 pin 没有替换。

本轮最终门禁与分层证据见[JSON收据](rust-agent-ui-strict-column-20261021.json)。源码几何测试不是 ClientKit 原生并排像素验收。Run 文档/Composer/chrome/selection 叠层、完整 rail、归档可见性、全部 pane 显示规则、真实三端阅读、物理 IME/VoiceOver仍按[审查清单](clientkit-view-layout-review-20261020.md)继续，UI保持实验状态。RunSurface只作协议/阅读测试宿主。
