# AgentKit × TekesUI `AppHost` 容器架构规范

状态：正式设计基线（消费不可变公开版本 `TekesUI v0.4.12` / `427c0e5d9e6fcdb0d71d203c402decdc05761df9`）。

## 1. 四层所有权与边界

| 层 | Crate / 归属 | 拥有 | 明确不承担 |
|---|---|---|---|
| **TekesUI** | `tekes-ui-controls`, `tekes-ui-app-host`, `tekes-ui-settings`, `tekes-ui-render`, `tekes-ui-platform-macos` | `AppWindow`、`PresentationOptions`（`nav_bar` / `focus` / `others` / `OthersGroup` / `ContainerFactory`）、`ContentHost`、`TextFieldContent`、受控草稿事务、`ReadingController`、前插阅读锚点、窗口级 `NavigationOptions` / `SettingsWindow` / `WindowWake`、Metal/CoreText/AX 桥接 | Agent 会话生命周期、SEP/WSE 协议、草稿/提交身份归属、第二套 Run 状态机 |
| **AgentKit Headless & Presentation** | `crates/{contract,session,presentation,web}` | `SessionRuntime`、`RunController`、`RunSnapshot`、`SideChatController`、`SummaryFacts`、`AgentTask`、`ChildRun`、`WorkspaceServiceEndpoint` 契约、批注与提交幂等/陈旧防护 | UI 控件布局、窗口管理、平台原生事件 |
| **AgentKit UI Components & Bindings** | `ui/components` (`tekes-agent-components-ui`), `ui/bindings` (`tekes-agent-ui-bindings`) | 槽位无关的 Agent `Content` 组合（`TranscriptSurface`, `ComposerSurface`, `SidebarSurface`, `SummarySurface`, `TasksQueueSurface`, `FilesSurface`, `ChangesSurface`, `ArtifactSurface`, `ChildRunSurface`, `SideChatSurface`）、响应式宽度布局、Fenced Bindings、`WorkspaceBinding` | `AppWindow`、`SettingsWindow`、`objc2`、具体 `tekes_kernel` 进程启动、与 `AppHost` 竞争的自建 `SplitView + TabStrip` 外壳 |
| **AgentKit `AppHost` 适配层** | `ui/apphost` (`tekes-agent-apphost`) | `SharedAgentWorkbench`（无跨容器借用冲突的共享协调器 + 意图信箱 + `WindowWake` 分发）、`ManagedAgentContainer`（`ContainerContent` 实现）、各标准 `LocalContainer` / `ContainerFactory` 构造器、`AgentSettingsOwner`、`ComponentDefinition` V2 适配descriptor | 宿主的整体窗口布局（`PresentationOptions` 决策）、业务主视图、凭据与系统权限 |
| **Desktop Host / Consumer** | `ui/demo`（形态 A）及外部宿主如 `ui/consumer`（形态 B） | `AppWindow` 装配（决定哪些容器进 `nav_bar` / `focus` / `others`）、业务主视图、认证/端点配置、自定义领域 `ArtifactViewerProvider` 注入 | 复制 Agent 状态机或直接读取 harness 私有库 |

> **关键原则**：AgentKit **不再**交付自管的 `SidePanel(SplitView + TabStrip)` 外壳。窗口三区布局、Tab/Group 切换、`Others` 列表堆叠与 `focus ↔ others` 原位交换完全由 `TekesUI` 的 `tekes-ui-app-host` 承担；AgentKit 交付标准、可复用、槽位无关的 `Container` 单元。

## 2. 标准 `AgentKit` 容器目录

所有容器单元同时暴露底层 `Content` 视图组件与 `ui/apphost` 标准 `Container` / `ContainerFactory` 构建器：

| 容器单元 | 对齐 ClientKit 能力 | 推荐 `AppHost` 槽位 | `Others` 呈现模式 (`ContainerPresentation`) | 实例策略 (`ContainerFactory`) | 关键 `AppHost` 能力对接 |
|---|---|---|---|---|---|
| `AgentRunContainer`（亦可拆分为 `TranscriptContainer` + `ComposerContainer`） | `TranscriptRendering` + `Composer` + 内联审批/提问卡 | `focus` 或 `others`（可原位互换） | `Tab` | `Single` | `focus_switch`、`search_entry`（窗口 `Cmd+F`）、`prepare_resources`（内联图片纹理）、`WindowWake` |
| `SessionSidebarContainer` | 会话列表、搜索过滤、文件夹分组、Pin/Rename/Archive | `nav_bar`（或 `others`） | `Tab` | `Single` | `take_navigation_request()` 接入窗口 `NavigationOptions` 历史前进/后退 |
| `SummaryContainer` | 会话摘要、Goal/Context、Usage 统计、产出/子代理索引 | `others`（亦可换入 `focus`） | **同时支持 `Tab` 与 `List { height }`** | `Single` | 在 `OthersTab::List` 中与 `TasksQueueContainer` 纵向堆叠 |
| `TasksQueueContainer` | 排队消息（Edit/Remove）+ 后台 `AgentTask` 状态列表 | `others` | **`List { height }` 优先**，亦支持 `Tab` | `Single` | epoch/connection 防护；点击子代理投递 `OpenChildRun` |
| `WorkspaceFilesContainer` | 项目文件树、路径模糊搜索、Git 状态标记 | `others`（可换入 `focus`） | `Tab` | `Single` | 激活文件投递 `OpenArtifact` 或附加入 `Composer` |
| `WorkspaceChangesContainer` | Working Tree / Turn Changes、统一 Diff、revision 校验提交 | `others` 或 `focus` | `Tab` | `Single` | `focus_switch` 窄/宽切换；行选区回传 `ArtifactSelectionReference` |
| `ArtifactViewerContainer` | 代码/文本/Markdown/图片预览、选区批注回传、宿主领域扩展 (`ArtifactViewerProvider`) | `others`（由 `ContainerFactory` 自动形成 `OthersGroup`） | `Tab` | **`Multiple`** | 惰性按文件打开、关闭自动注销 GPU 纹理与 staging 资源 |
| `ChildRunContainer` | 只读/有界历史子代理轨迹 + 分页加载 + 缺失提示 | `others`（自动 `OthersGroup`） | `Tab` | **`Multiple`** | 按 `task_id` 创建独立只读副本，绝不伪造主运行 `Composer` |
| `SideChatContainer` | 独立草稿/IME 的侧边临时分支对话 | `others`（自动 `OthersGroup`） | `Tab` | **`Multiple`** | `ComposerDestination::SideChat` + `PaneAccess` 门控 + 关闭安全清理 |
| `AgentSettingsOwner` + `HarnessSettingsContainer` | 外观/缩放偏好 + Agent 行为偏好 + Harness 命名空间设置 | `SettingsWindow` (`SettingsOwner`) + 可选 `others` Tab | 独立设置窗 / `Tab` | 单例 `SettingsOwner` | 对接 `SettingsOwner::with_display_preferences`、`settings_undo` 与 CAS `revision` |

## 3. 跨容器共享状态与生命周期规则

1. **零兄弟借用（Zero-Sibling-Borrow）**：严禁在 `with_host`、`on_action`、`on_command` 或 `prepare_for_render` 内持有 `AgentWorkbenchState` 的可变借用跨越另一个容器的 `ContentHost` 操作；严禁直接访问兄弟容器的 `ContentHost`。
2. **共享协调器模式（`SharedAgentWorkbench`）**：
   - 容器事件（如 Sidebar 选中会话、Files 打开文件、Artifact 回传代码选区、Composer 发起发送）仅把紧凑动作写入 `SharedAgentWorkbench` 的命令队列 / 状态版本并释放 `RefCell` 借用，随后调用所有已注册 `WindowWake::wake()`。
   - 每个 `ManagedAgentContainer` 在下一次 `prepare_for_render()` 时检查版本戳：仅更新自己的 `ContentHost`。
3. **隐藏态节流（`set_active`）**：当某容器处于未选中的 `Others` Tab 或被遮盖关闭态时，仅记录最新 `source_revision`，推迟重建 UI 树直到重新激活并进入渲染前置流程。
4. **原位槽位交换与响应式布局**：
   - `focus_switch` / `PresentationState::promote` 仅移动容器 ID 槽位，绝不克隆或销毁重建 `ContentHost`。
   - 所有组件以 `width < 520.0` 为 `Compact` 断点、`width >= 520.0` 为 `Expanded` 断点，仅重排视觉控件而不更换底层 `TextFieldContent` / `ReadingController` 实例，确保槽位互换后 `mount_identity()`、滚动锚点与中文 IME `preedit` 完全保留。

## 4. 双形态桌面 `AppHost` 验收定义

- **形态 A（Agent 工作台布局 — `ui/demo`）**：
  - `nav_bar`: `SessionSidebarContainer`（提供 `NavigationOptions` 工具栏目标）
  - `focus`: `AgentRunContainer`（`Transcript + Actionables + Composer`）
  - `others`: `[SummaryContainer(List), TasksQueueContainer(List), WorkspaceFilesContainer(Tab), WorkspaceChangesContainer(Tab), HarnessSettingsContainer(Tab)]` + `ContainerFactory(Multiple)`（`ArtifactViewer`, `ChildRun`, `SideChat`）
- **形态 B（宿主业务主导布局 — 独立公开消费者 `ui/consumer`）**：
  - `nav_bar`: 宿主领域大纲树（`domain-outliner`）
  - `focus`: 宿主领域画布（`domain-canvas`）
  - `others`: `AgentRunContainer` + `SummaryContainer(List)` + `TasksQueueContainer(List)` + `WorkspaceFilesContainer` + `WorkspaceChangesContainer` + 注入自定义 `.cadspec` 查看器的 `ArtifactViewerContainer` 工厂
  - **必测项**：通过 `workspace_switch(true)` / `focus_switch` 将 `AgentRunContainer` 从 `others` 换入 `focus` 再换回，断言 `ContentHost` 挂载身份不变、滚动锚点不变、中文 IME `preedit` 组字不中断。
