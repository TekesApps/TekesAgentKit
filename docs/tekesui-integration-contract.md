# AgentKit × TekesUI：AppHost 容器接入契约

状态：基于不可变公开 `TekesUI v0.4.1`（`4ed5b2912e34312a0e0d9e9780d09482b58a0cc6`）的原生 `AppHost` 容器体系，完整架构定义见 [AppHost 架构规范](tekesui-apphost-architecture.md)。

## 依赖与隔离

- Headless / Web 工作区（根目录 `Cargo.toml`）通过 `exclude = ["ui"]` 保持完全独立，不引入任何 `tekes-ui-*` 依赖。
- `ui/Cargo.toml` 明确固定指向 `TekesUI` 公开不可变 commit `4ed5b2912e34312a0e0d9e9780d09482b58a0cc6`（tag `v0.4.1`），记录于 `ui/tekesui-source.json` 并由 `scripts/check-ui-boundaries.py` 静态校验，不依赖未提交的兄弟工作树。
- 严格禁止任何 Swift 源码或工具链；`ui/` 全工作区强制 `#![forbid(unsafe_code)]`（工作区 `unsafe_code = "forbid"`），平台 `objc2` 封装完全留在 TekesUI 内部。

## 所有权与数据流

| 层 | 拥有 | 不承担 |
|---|---|---|
| TekesUI (`AppHost` / `Controls` / `Settings`) | `AppWindow`、`PresentationOptions`（`nav_bar` / `focus` / `others` / `OthersGroup` / `ContainerFactory`）、`ContentHost`、`TextFieldContent`、受控值事务、`ReadingController`、前插阅读锚点、`WindowWake`、Metal/CoreText/AX | Session、发送回执、SEP/WSE、Agent 状态机 |
| AgentKit Presentation & Bindings | Session scope、`RunSnapshot`、草稿、`SideChatController`、`SummaryFacts`、`AgentTask`、`WorkspaceBinding`、epoch/revision 防护 | 通用窗口分栏、TabStrip/SplitView 外壳 |
| AgentKit UI Components & `AppHost` Adapters | 槽位无关组件与 `ContainerContent` 包装、`SharedAgentWorkbench` 共享意图通道、响应式窄/宽重排、`AgentSettingsOwner` | 业务窗口主布局决策、凭据、系统权限 |
| Desktop Host（形态 A `demo` / 形态 B `consumer`） | `PresentationOptions` 槽位编排、业务主视图、Tokio 运行时、自定义 `ArtifactViewerProvider` | 私有 AppKit 调用、复制 Run 状态机 |

## 焦点、IME、草稿与槽位热交换

- 每个 destination（主会话 `Run`、各 `SideChat`）保留独立的 `TextFieldContent` 与阅读视图实例；`preedit` 期间不写入 Agent 草稿，禁止 `Send`、会话切换与外部值覆盖。
- `AppHost` 执行 `focus ↔ others` 互换或隐藏恢复时仅改变布局位置，`ManagedAgentContainer` 保持底层 `ContentHost`、`ReadingController` 锚点及活跃 `preedit` 原样存活，仅按容器宽度断点自适应工具栏与卡片排布。

## 验证分层

证据始终分六层独立记录，互不替代：
1. **静态边界与 Root 回归**：`check-headless-boundaries.py`、`check-ui-boundaries.py`、`scripts/tests`、Root `cargo test`。
2. **Headless 组件与重放**：`SharedAgentWorkbench` / `ManagedAgentContainer` 生命周期、前插锚点、受控编辑、Fenced Actions 与双形态容器矩阵。
3. **真实 Kernel SEP → AppHost GPU 离屏渲染**：真实 Kernel 进程经 `SessionRuntime` 驱动 `AppHost` `HostedWindow::render_headless_rgba8` 输出 PNG。
4. **原生窗口合成自测（`--native-selftest`）**：使用 `tekes-ui-platform-macos` 公开 selftest 接口驱动真实 macOS `AppWindow`，验证无 `--frame-pump` 的 `WindowWake` 刷新、合成 IME `Preedit`/`Commit` 与 `focus_switch` 互换。
5. **双公开消费者验证**：形态 A（`ui/demo`）与形态 B（独立公开消费者 `ui/consumer`）各自装配并验证 `OthersTab::List`、`ContainerFactory` 多实例及跨槽位交换。
6. **真机人工与硬件验收（独立标记 `not_run` / 人工单独执行）**：真实物理系统中文输入法候选窗、macOS VoiceOver 真人走查与 120Hz ProMotion 真机手感性能。
