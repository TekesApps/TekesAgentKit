# SidePanel 正文视口与宿主顶部净空

对照当前 ClientKit `ClientSidePanelHostView.layout` 与 `SidePanelContentFrame.inset`，以及 Tekes `SidePanelHostView` 的 window contentTopInset：AgentKit 接收宿主提供的顶部净空，在其下保留有界正文区域。选中的 Agent tab 正文四边使用 TekesUI `Spacing::Regular`；launcher/父投影等待时不加卡片 inset。短正文不使 pane 收缩。窗口/标题栏测量仍归宿主。

公开 `SidePanel::set_layout(SidePanelLayout { content_top_inset })` 验证有限非负输入，超大净空通过原 SDK `ContainerLayout` 安全折叠视口。只改变布局，不替换原内容/token/editor。`layout_geometry()` 是最近一次 arrange 的 pane/canvas/body/visible_body，尚未 arrange 或输入更新时返回 None；这不是成功 render 事实或 reading authority，不得用它替换 RunView 的正式阅读事务。

第二公开 consumer 通过 `set_panel_layout` 接入。最小用法：

```rust
panel.set_layout(SidePanelLayout {
    content_top_inset: host_titlebar_clearance,
})?;
// 更新原 SidePanelInput，按原 PanelContentToken 安装内容；
// 由同一个 ContentHost 在后续 render 统一 layout/reading。
```

此处仅为 Agent pane 的布局组合，所有 inset、clip 归一化和 stack 放置继续调用 TekesUI；没有手绘边框、蒙版或新滚动引擎。原 demo 的 inline tab/action chrome 仍是实验入口，不是 ClientKit 的窗口 toolbar。完整 tabs/chrome 对齐继续跟踪 #36/#14。

ClientKit 的普通 pane 还包含 documentCanvas、圆角子树裁切及 descendant focus 边框。当前精确公开 SDK `09b4bab44b6dfa65487250ea816b610b9df59095` 的 SurfaceView 仅画基础面，Scene 仅公开矩形 clip，未提供等价原 Content 容器。已提 [TekesUI #45](https://github.com/TekesApps/TekesUI/issues/45)，附公开源码依据、属性/行为和独立 consumer 验收；AgentKit 不复制这套通用容器。因此本阶段不能宣称完整 SidePanel 外观完成。

验收证据保留在 `.runtime-fixtures/sidepanel-layout-20261022/`，包括失败与修正记录。此前程序/收据保持原归属，不改写其验收范围。复用既有 `/tmp/agentkit-layout-text-292-target`，无新 target。

## 本阶段验收

445项 UI workspace 测试（包含第二公开 API consumer）、strict all-target Clippy、fmt、边界检查和 native build 通过。新增布局测试使用非零 pane origin、32pt 宿主净空、regular body inset、短内容固定底边、原内容地址/token不变、非法输入拒绝与超大净空折叠。

同一冻结程序 SHA256 `bd1da513183def671b33ee0b9362a4281ab5b05787ef1ffb8a0412e601dc20be` 保留在 `.runtime-fixtures/AgentKitPanelLayoutCheck.app`：

- 原生 projection专项在主编辑器中文组字期间设置28pt净空，验证真实排版的canvas/body inset；等待/ready、A/B/A后 pane draft/pointer 与主编辑器baseline保持，旧token拒绝，无提交。
- 原生混合Transcript专项通过实际分页20→44、前插锚点、A/B阅读与草稿、重连、普通Start/Latest/Jump及原editor/focus/marked baseline。
- 原生截图 `native.png` 已检查消息角色方向、独立草稿、正文四边净空与顶部净空。它不是同内容ClientKit配对图，也不代表完整tabs/card视觉验收。

上述为 owned fixture 和合成wheel/preedit，不代表本轮真实Native/Pi/DSH、实体键盘、候选窗或VoiceOver全量验收。系统简体拼音的最近证据仍属于前一冻结程序，见上一阶段记录，不混作新二进制证据。本阶段未新增SDK依赖或重编译SDK，没有清理用途不明确的共享缓存；删除字节为0。历史失败日志、固定源码、验收程序和证据均保留。
