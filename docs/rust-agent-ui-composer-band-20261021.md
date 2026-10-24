# Composer 输入行与单一主动作（实验）

正式 Composer 改为 context → 输入 band → 可选 setup 行。编辑正文与主动作/Add 同属输入 band，主动作在 Add 左侧；不再把发送、停止和所有入口排成正文下方的一条工具栏。正文使用 TekesUI 公开的 Content presentation，保留原 TextFieldContent 的编辑、受控草稿、撤销、选区、IME 与滚动；外层仅由 SurfaceView 按真实 backing scale 绘制。

对照只读 ClientKit 的 `ComposerCardView.layout/fittingHeight`、`ThreadComposerView.ComposerPrimaryActionPolicy`、`ComposerMetalEditor.Metrics`、`UserControlMetrics`。这里只修改 Rust；没有改 ClientKit/Tekes 或 TekesUI 源输入。

## 已接入规则

- 只挂载一个主动作。canonical run 可取消且没有 canonical 可提交输入时显示 Stop；有可提交新输入时显示 Send，保留 Queue 提交语义。空闲运行不受历史未完成行影响。pane gate 只收紧权限，不能通过改变按钮种类授予能力；组字仍不能 Send。
- Send/Stop 显示分别保留原数值 ID 和原 Content/Button owner，隐藏期间清理库的旧命中/capture 布局；重新显示不重建按钮。拿到旧 Send 动作后发生状态切换，动作不会被解释成 Cancel，也不会发送。
- context 为空不占位；没有模型/预设入口时不保留空 setup 高度。原 context/editor 实例与独立会话草稿保持不变。
- 小窗口同时限制原编辑器的公开 Grow 上限，使用库的内部滚动；卡片高于可用区域时保留底部输入/主动作，优先裁剪上方内容。卡片自身的完整折叠/滚动呈现仍需与 Source stacked family 对齐。
- 先测量库的真实按钮，再为正文预留宽度；单行编辑 envelope 与动作共用垂直中心，增长输入时动作靠最后可见行。短编辑器和多行正文都保持不重叠。最大十行随宿主字体倍率更新；文字测量、软换行和内部滚动继续归库。
- `Composer::primary_action_kind()/primary_action_id()` 供宿主查询当前共享策略；`layout_geometry()` 返回最近安排的 context/input/editor/primary/attachment/setup 及原 publication/destination。更新后重新 arrange 才能得到新几何；宿主还需检查 pane 是否呈现。第二公开 consumer 提供 `arranged_composer_layout`，不再自行估算主动作位置或编辑器高度。

基础 Surface、正文、文字按钮仍消费 TekesUI 默认样式及 tokens。库的文字按钮尺寸与 ClientKit 原生 glyph slot 不同，所以预留使用库的实际测量值，不能把本文的结构修正宣称为完整像素/图标对齐。

## 仍需完成

模型/预设目前仍在实验 inline setup 行；ClientKit 当前 Run 把 setup chip 挂入宿主窗口 toolbar，后续应通过明确 host input/callback 组合共享组件。当前 Add 是已有附件入口，完整 Shortcut Palette、chip/strip/notice、rewind 和图标主动作还没有全部对齐。实际 Source glyph slot、完整 Composer 视觉与整页等条件对照继续保持未验收。

[TekesUI #30](https://github.com/TekesApps/TekesUI/issues/30)记录复合输入面到原编辑器的焦点路由需求。只读2b16上的消费探针 `scripts/probes/agentkit-composer-padding-focus.rs` 复现：未聚焦时点父输入面留白不能获取原编辑器焦点；已经组字时点相同留白会保留焦点和 preedit。后一行为已由库保护，不能误报为 IME 回归。它是已知边界复现，不是修复通过；未把候选结果扩展为默认8cf运行时验收，也不在消费侧造第二个编辑器或转发鼠标来规避。

本轮没有另写通用编辑器、按钮输入状态机、焦点或动画。原生 fixture 脚本按单一主动作更新：空输入时先 Stop，再输入未发送草稿，不再假定新输入可提交时还有第二个 Stop 按钮。阶段结果需分别核对发送次数、取消与独立草稿，不能仅看点击完成。

新增测试包含24组真实 CoreText 的字号/主题/宽度/长短中文组合、空 setup、单一主动作及旧动作拒绝、按钮 owner 保留和第二公开 consumer。完整测试/候选阅读事务及原生证据见同名 JSON 收据；它们不替代真实三端阅读/分页、物理键盘与候选窗、VoiceOver 或 ClientKit 配对视觉验收。
