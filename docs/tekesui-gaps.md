# TekesUI 消费侧缺口与上游闭环状态

更新日期：2026-11-20。基准从开发快照迁移至不可变公开版本：`TekesUI v0.4.1`（commit `4ed5b2912e34312a0e0d9e9780d09482b58a0cc6`，`ui/tekesui-source.json`）。

| ID | 历史缺口 | 上游状态 (`TekesUI v0.4.1`) | AgentKit `ui/` 消费方式与剩余边界 |
|---|---|---|---|
| TKUI-001 | 缺少后台 `Send + Clone` UI 唤醒代理 | **已关闭** ([TekesUI #15](https://github.com/TekesApps/TekesUI/issues/15))：`tekes_ui_platform_macos::WindowWake` 与 `ContainerContent::start_process(&mut self, wake: WindowWake)` 已公开可用 | `SharedAgentWorkbench` 注册 `WindowWake`，流式输出、状态变更与异步读回按需唤醒窗口，**正式移除 `--frame-pump` 轮询** |
| TKUI-002 | 历史前插阅读锚点缺失 | **已关闭** ([TekesUI #16](https://github.com/TekesApps/TekesUI/issues/16))：`ReadingController::set_document` / `update_document` + `ReadingDocumentVersion` + `TextRangeMapping` 已公开 | `LoadEarlier` 前插旧历史时计算保留段 UTF-16 区间映射，交由 `ReadingController` 保持逻辑阅读锚点，AgentKit 不自算像素补偿 |
| TKUI-003 | 文本命令、修饰键与 IME 消费语义 | **已关闭** ([TekesUI #17](https://github.com/TekesApps/TekesUI/issues/17))：`TextFieldContent` 公开 `TextSubmitPolicy` / `TextTabPolicy`、`ContentKeyEvent` 与 IME disposition | `Composer` 支持多行输入、`Cmd+Enter` 发送（可配置）及 `composing()` 保护；真机物理拼音/日文 IME 体验仍独立于合成测试单列 |
| TKUI-004 | 带目标与来源的控件动作事件 | **已关闭** ([TekesUI #21](https://github.com/TekesApps/TekesUI/issues/21))：`ContentHost::set_action_events(true)` 与 `drain_action_records()` 输出 `ContentActionRecord { action, mount, payload }` | `ManagedAgentContainer` 统一通过 `LocalContainer::on_action` 消费类型化动作，移除按钮 `activations` 计数轮询 |
| TKUI-005 | 受控编辑值同步事务 | **已关闭** ([TekesUI #18](https://github.com/TekesApps/TekesUI/issues/18))：`TextSurface::apply_external_value`、`DraftRevision`、`TextEditBaseline`、`ExternalSelection`、`ExternalUndo`、`ExternalValueResult` | 远端草稿与发送清空经基线/版本事务同步，组字期间安全拒绝外部替换并保留原生撤销栈 |
| TKUI-006 | 指针选择元数据透传 | **已关闭** ([TekesUI #19](https://github.com/TekesApps/TekesUI/issues/19))：`PointerEvent` 携带 `PointerButton`、`InputModifiers`、`click_count` | 直接委托 `TextFieldContent` / `RichContent` 执行 Shift-click 与多击词段选择，真机鼠标手感单列记录 |
| TKUI-007 | 可配置文本视口与多行尺寸约束 | **已关闭** ([TekesUI #20](https://github.com/TekesApps/TekesUI/issues/20))：`TextViewport::{Form, Fixed, Fill, Grow { min, max }}` 与 `TextFieldContent::read_only_content` / `RichContent` | `Composer` 消费 `TextViewport::Grow`，阅读器消费 `RichContent` / `ReadingController`，不再手写高度覆盖 |

## 仍需独立分列报告的验证边界

即使上述 7 项通用 UI API 已在 `TekesUI v0.4.1` 闭合并被 `TekesAgentKit` 消费，下列人工/硬件维度仍**不可由合成事件或离屏渲染替代**，报告证据时必须严格区分：
- 人类真实系统中文/日文物理输入法候选窗联调（Synthetic `Preedit`/`Commit` 通过 ≠ 物理输入法已验收）；
- macOS 系统 VoiceOver 真实朗读与焦点走查（AX 节点语义树完整性校验 ≠ 真人 VoiceOver 验收）；
- 120Hz ProMotion 真机物理帧率 pacing 与长会话人工体感滚动。
