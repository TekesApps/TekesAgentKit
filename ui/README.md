# TekesAgentKit Rust UI (`ui/`) · `TekesUI v0.4.7` `AppHost` 组件库

复用 `TekesAgentKit v0.5.0` 的 headless 协议与表示层（`tekes-agent-contract`、`tekes-agent-session`、`tekes-agent-presentation`、`tekes-agent-endpoint-kernel`、`tekes-agent-web`），通过不可变公开 git revision（[tekesui-source.json](tekesui-source.json)，`TekesUI v0.4.7` / `563c2a505fe8bbf149930b1e3ae39778900c2d7f`）原生消费 `tekes-ui-app-host`、`tekes-ui-controls`、`tekes-ui-settings` 与 `tekes-ui-render`。

- **五层 / 五 Crate 独立工作区**：`components`、`bindings`、`apphost`、`demo`（形态 A）、`consumer`（形态 B，仅消费公开 API），统一 `unsafe_code = "forbid"`，不包含 Swift 源码或 `--frame-pump` 轮询，所有异步增量由 `WindowWake` / `AppHostWakeHub` 按需驱动。
- **架构与契约文档**：[AppHost 架构规范](../docs/tekesui-apphost-architecture.md)、[集成契约](../docs/tekesui-integration-contract.md)、[TekesUI 缺口追踪](../docs/tekesui-gaps.md)。
- **工作区隔离**：`ui` 继续保持在根目录 `Cargo.toml` 的 `workspace.exclude` 中，所有 UI Crate `version = "0.0.0"`、`publish = false`。

## 1. Crate 结构与标准容器目录

| Crate / 包 | 职责 | 关键导出 |
|---|---|---|
| `components` (`tekes-agent-components-ui`) | 槽位无关的纯 UI `Content` 组合、disjoint `ComponentId` 命名空间、`Compact (< 520px)` / `Expanded (>= 520px)` 响应式布局与 `AgentContainerShell` | `TranscriptSurface`、`ComposerSurface`、`SidebarSurface`、`SummarySurface`、`TasksQueueSurface`、`WorkspaceFilesSurface`、`WorkspaceChangesSurface`、`ArtifactViewerSurface`、`ChildRunSurface`、`SideChatSurface`、`HarnessSettingsSurface` |
| `bindings` (`tekes-agent-ui-bindings`) | 平台/渲染无关的状态绑定、SEP/WSE Fixture、epoch / CAS 防护与 `cap-std` 只读文件系统沙盒 | `RunWorkbenchBinding`、`WorkspaceBinding`、`ChildRunBinding`、`SideChatBinding`、`HarnessSettingsBinding`、`FixtureSessionEndpoint`、`DeterministicWorkspaceEndpoint`、`ReadOnlyFsWorkspaceEndpoint` |
| `apphost` (`tekes-agent-apphost`) | `SharedAgentWorkbench`（零兄弟借用调度）与 `ManagedAgentContainer` 标准 `LocalContainer` / `ContainerFactory` / `SettingsOwner` 构建器 | `AgentRunContainer`、`TranscriptContainer`、`ComposerContainer`、`SessionSidebarContainer`、`SummaryContainer`、`TasksQueueContainer`、`WorkspaceFilesContainer`、`WorkspaceChangesContainer`、`ArtifactViewerContainer` + `ArtifactViewerProvider`、`ChildRunContainer`、`SideChatContainer`、`HarnessSettingsContainer`、`build_agent_settings_owner` |
| `demo` (`tekes-agent-ui-demo`) | **形态 A（Agent 工作台）**：`nav_bar=agent-sidebar`、`focus=agent-run`、`others=[Summary(List), TasksQueue(List), Files, Changes, HarnessSettings]` + `Multiple` 工厂（`ArtifactViewer`、`ChildRun`、`SideChat`） | 二进制参数：`--offscreen <path.png>`、`--theme light\|dark`、`--swap-focus`、`--endpoint-config <json>`、`--native-selftest` |
| `consumer` (`tekes-agent-ui-consumer-check`) | **形态 B（宿主业务主导工作室）**：`nav_bar=domain-outliner`、`focus=domain-canvas`、`others=[AgentRun, Summary(List), TasksQueue(List), Files, Changes]` + 定制 `CadSpecViewerProvider` | `DomainStudioRuntime`、`CadSpecViewerProvider`、`build_topology_b_app`、`examples/render_topology_b.rs` |

## 2. 构建、静态边界检查与运行命令

从仓库根目录运行（串行共享 Cargo target）：

```sh
# 静态所有权、unsafe=forbid 与 TekesUI v0.4.7 Pin 校验
python3 scripts/check-ui-boundaries.py

# 全量 lint 与测试（Phase 0-3 + 形态 A/B + Metal 离屏 PNG）
cargo fmt --manifest-path ui/Cargo.toml --all --check
cargo clippy --manifest-path ui/Cargo.toml --workspace --all-targets --locked -- -D warnings
cargo test --manifest-path ui/Cargo.toml --workspace --locked

# 形态 A（Agent 工作台）离屏 PNG 与原生 macOS 窗口自检
cargo run --manifest-path ui/Cargo.toml -p tekes-agent-ui-demo --locked -- \
  --theme light --offscreen /tmp/apphost-topology-a-light.png
cargo run --manifest-path ui/Cargo.toml -p tekes-agent-ui-demo --locked -- \
  --theme dark --offscreen /tmp/apphost-topology-a-dark.png
cargo run --manifest-path ui/Cargo.toml -p tekes-agent-ui-demo --locked -- \
  --native-selftest

# 形态 B（CAD Studio 宿主主导布局）双态 Metal 离屏渲染
cargo run --manifest-path ui/Cargo.toml -p tekes-agent-ui-consumer-check --example render_topology_b --locked -- \
  /tmp/apphost-topology-b-domain-focus.png \
  /tmp/apphost-topology-b-agent-promoted.png
```

真实 Native Kernel v4 连接配置模板（令牌仅从 `bearerEnv` 读取）：

```json
{"endpointId":"native-ui","url":"http://127.0.0.1:PORT","bearerEnv":"MY_EXPLICIT_BEARER","sessions":["SESSION_ID"],"cwd":"/absolute/workspace"}
```

```sh
tekes-agent-ui-demo --endpoint-config /absolute/native.json --offscreen /tmp/apphost-real-kernel.png
```

## 3. 六层验证矩阵与诚实边界声明

| 验证层 | 已验证范围与实测证据 | 明确不越界宣称（Not Proven Here） |
|---|---|---|
| **Static 边界** | `scripts/check-headless-boundaries.py`、`scripts/check-ui-boundaries.py`、`cargo clippy --workspace --all-targets --locked -- -D warnings`（root & `ui/`）、`unsafe_code = "forbid"`、TekesUI `563c2a505fe8bbf149930b1e3ae39778900c2d7f` pin 校验 | 仅静态依赖与边界约束，不代表运行时覆盖 |
| **Replay（确定性回放）** | `phase0_lifecycle`、`phase1_core_trio`、`phase2_workspace_inspectors`、`phase3_multiagent_settings`、`reading`、`run_flow`：覆盖 `WindowWake` 延期同步、`ReadingController` 前插锚点、`FixtureSessionEndpoint` 流式/取消/分页/Actionable、epoch 队列屏障、`ChildRun` `Missing`、`SideChat` 草稿隔离、CAS 与秘密字段 `••••••••` 掩码、`cap-std` 逃逸拦截 | Fixture 协议确定性回放，不替代真实 LLM 与线上网路延迟 |
| **Integration（真实协议 / 双形态）** | 隔离临时 `HOME` + 临时端口真实 `TekesKernel v0.9.23` (`tekes-supervisor` / `tekes-worker`) 启动与 `tekes-agent-headless` 建会/对话，经 `--endpoint-config` 连通 `SessionRuntime` + `RunController` 渲染出真实会谈截屏；独立公开消费者 `ui/consumer` 通过 `apphost_topology_b`（`CadSpecViewerProvider` + 零重建 `promote` 槽位互换） | Kernel 冒烟使用本地合成 SSE responses provider（零付费模型调用）；Pi/DSH UI 原生宿主演示待独立接装 |
| **Native UI（原生窗口 & GPU 渲染）** | macOS `AppWindow` 真机窗口运行 `--native-selftest`（`Resize`、`WindowDark`/`WindowLight`、`ClickPublished` 命中焦点）并返回 `SELFTEST_OK`；Metal `Renderer::new_headless()` 生成并目视检查形态 A Light/Dark 与形态 B Domain Focus / Agent Promoted 四张 `(1440x900)` PNG 及 Real Kernel Smoke PNG | `--offscreen` 为固定分辨率离线 Metal 渲染，不能等同多显示器缩放/全屏幕窗口管理压力测试 |
| **IME / Accessibility（合成输入与辅助功能）** | 原生窗口 `--native-selftest` 注入合成 `Preedit("nihao")` -> `Text("你好世界")`；单元/集成测试注入 `ContentEvent::Preedit` 验证组合态 `ExternalValueConflict::Composition` 在宽窄栏切换及 SideChat 隔离下的保护 | **未在物理硬件运行真人中文输入法候选窗选词，未实测 VoiceOver 朗读听感与完整辅助导航链路** |
| **Performance（性能 / 刷新）** | 全部由 `WindowWake` 与 `Render` 脏版本/非激活延期驱动，无 `--frame-pump` 空转定时器；多容器共享零兄弟借用协调器 | **未进行 120Hz ProMotion 硬件掉帧采集、超万条历史滚动帧率 Profiling 与长时内存泄漏压测** |
