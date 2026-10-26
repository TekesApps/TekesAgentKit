# 正式 RunView 的共享会话列（实验）

> 本文保留2026-10-20的阶段证据；2026-10-21严格列规则的修正见[新记录](rust-agent-ui-strict-column-20261021.md)，旧Relaxed净空、窄列收缩和拒绝比例策略已由新实现替代。

参照当前 ClientKit 的 `Composer/Run/RunOverlay.swift::ClientRunColumnLayout` 和 `Composer/RunUserControlsGraph.swift::viewDidLayout`，正式 Rust RunView 新增公开 `RunColumnLayout` 以及 `set_column_layout` / `column_layout`。Transcript viewport、Composer、分页/等待/状态和宿主附加的 Run controls 共用一条水平列；宽窗不再把正文/编辑器铺满整个 pane。改变布局保留原控件、编辑器、消息行、操作回执和 presentation。

```rust
use tekes_agent_components_ui::{RunColumnLayout, RunScope, RunView};
let mut run = RunView::standard(100, 104..400, RunScope::main("endpoint", "session"))?;
// 默认为居中 reading column，maximum_width = 770（ClientKit 的布局上限）。
run.set_column_layout(RunColumnLayout::FullWidth)?;
// 子面板可显式传入非对称边距；不更换数据源或另建视图。
run.set_column_layout(RunColumnLayout::Insets { leading: 0.15, trailing: 0.08 })?;
```

Reading / FullWidth 的两侧净空使用当前 TekesUI `Spacing::Relaxed`，而非复制 AppKit 的 rail 光学补偿或固定边距。最大阅读宽度是 Agent 的布局约束，宿主可显式选择其它正数；颜色、文字、圆角、控件背景和交互始终由 TekesUI 提供。列居中、边距归一化与 clipping 使用库的 `ContainerLayout` / `layout_measured_children`，没有复制通用布局引擎、焦点/IME 或滚动算法。

`Insets` 的 leading/trailing 为 pane 宽度比例，均须有限且在0..=0.45；Reading maximum_width 须有限且大于0。非法 setter 返回 Err，原 policy/控件保持不变。与 ClientKit 的输入归一化不同，Rust API 显式拒绝非法输入，避免宿主配置错误被静默修正。

## 验证范围

正式组件 CoreText 几何测试覆盖3种策略 × 360/760/1280点 × 1/1.5文字缩放 × 浅/深主题，共36组，检查两者共用边界、viewport 在 Composer 之前且不挤出底部。改布局/尺寸期间保留当前原 editor 的 TextEditBaseline、external value、焦点与合成 preedit，Transcript 行身份不变。它是几何和状态证据，不是 GPU 像素对照或真实输入法。

第二公开 API consumer 在挂载后切换策略，验证非法宽度/比例不改旧布局及实例，另一个 RunView 不受影响。完整 UI workspace、Clippy/fmt、实际不可变依赖和原生 fixture 的结果见[JSON 收据](rust-agent-ui-column-layout-20261020.json)。原生使用正式 `--transcript-content` 路径，而非行探针或 RunSurface；context/发送/合成组字/草稿隔离专项通过，不能扩大成三端真实阅读或物理 IME 验收。

本轮完整 UI 测试291项、严格 Clippy/fmt 与依赖输入检查通过。新增几何测试首次因漏导入 RunSnapshot 未编译，修正后通过。原生交互首次通过，但100ms截图是首帧前空窗，保留为未通过视觉证据；测试宿主将 Capture 排在已通过的1100ms原生组字断言后（1200ms），其它动作和断言时间不变。重建同一源码二进制并重跑，交互和已查看的正式 RunView 画面通过。未因此声称首次像素或完整布局验收通过。

## 仍未对齐的部分

- 当前纵向栈仍为 docked Composer，不是 ClientKit 的完整文档/Composer/chrome 叠层；完整回合 indicator rail、其导航与遮挡后的阅读几何未交付。
- 当前列在过窄 pane 内收窄，不复制 ClientKit 的320点最小列宽并让内容超出 pane；宿主仍需按其控件/窗口约束设置最小可用尺寸。更窄窗口、长本地化工具栏和完整弹层布局未验收。
- 没有引入持久 checkpoint、像素补偿或自制阅读状态。#22 阅读事务、#26 受控编辑预检、#25 富内容和真实三端阅读分页仍需完成正式接入/验收。
- 默认 TekesUI 仍是公开固定 `8cf0840790334ba224afc0595897c74231c712e6`，UI 仍 `0.0.0` / `publish=false`。共享会话列通过不代表全部 [ClientKit Views 对齐](clientkit-view-layout-review-20261020.md)完成，也不据此合并/打稳定 tag。

2026-10-21要求已改为**严格对齐**：上述320最小列宽、Relaxed净空及非法比例处理的差异现在都是必须修正项，不再视为最终可接受的消费侧策略。36组测试只证明当时实现的几何/状态稳定，不证明它已经严格匹配ClientKit。完整硬性清单见[对应Views审查](clientkit-view-layout-review-20261020.md)。
