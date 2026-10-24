# 反馈备注表单与 Popover 接收门槛

对照当前 ClientKit 的 `FeedbackNoteEditorView` / `FeedbackNoteEditorState` / `FeedbackNotePopover`，以及 Tekes `ClientAgentHostAdapters`：共享 Kit 拥有表单与反馈业务；宿主仍负责窗口/导航。仅将这些文件作为只读参考，没有 Swift 源码、工具链或跨仓库修改。

## 本轮表单改动

`TurnFeedback` 继续使用原 committed feedback frame、`FeedbackGesture` 和 binding CAS，不另建请求/草稿状态机。

- 新增 `FeedbackLabels.editor_title`，默认“评价”；`note` 继续是原文本输入的 AX 名称。现有 labels 的完整 struct literal 需补此字段，或采用 `..Default::default()`。`close` 保留原 API，默认文案改为“取消”。
- 打开时依次为标题、原多行 TextSurface、非空状态/错误、尾部动作。编辑区采用源 View 的命名尺寸：表单期望宽度400、编辑视口120；窄宿主保留库的测量和裁切规则。正文使用原 Content presentation 与 UiBody，不再叠加一层可见 Field 标签；标题、文字、间距与按钮外观继续由 TekesUI tokens/controls 提供。
- 尾部由原 SDK measured layout 将“取消 → 保存”靠右排列，保存选择原 Primary variant。没有复制按钮命中、焦点、键盘、样式或滚动实现；原控件和编辑器在 refresh/status/publication 更新中保持。
- 保存期间仍允许编辑原备注，符合源 View；重复保存由原 pending/once 屏障拒绝。取消、关闭后的失败、冲突、删除 rating、未确认结果与显式刷新继续使用原行为，未增加自动重试。普通 Enter 仍为多行输入，不成为业务保存动作。

## Popover 尚未接通

源 View 是锚定 transient Popover。当前公开精确 SDK `09b4bab44b6dfa65487250ea816b610b9df59095` 已有 `PopoverView::arrange/render_with_text_content` 和 Component overlay hooks，不能说没有定位/绘制能力；但独立 consumer 将同一原 Content 子树画入原 overlay pass 后，inside-content 事件回落到普通文档顺序，实际激活下层原按钮。

公开 API 最小复现 `ui/consumer/tests/support/popover_intake.rs` 使用同一 ContentHost；重叠区域由原 PopoverView 的 layout.content 提供，原弹层 ButtonContent 最后绘制。预期 Save `51002`，实测下层 Action `51003`。没有假 frame、第二 ContentHost、手动转发事件或焦点替代。这证明当前所尝试组合路径的输入/显示不一致，不将其表述为 PopoverView 内部算法 bug。

已提交 [TekesUI #49](https://github.com/TekesApps/TekesUI/issues/49)，需要原交互式 Content Popover 入口、输入优先级、原编辑器/Tab/AX、IME、外部点击/Escape、关闭原因、anchor/mount 与返回焦点生命周期。通用能力由 SDK 完成；AgentKit 只提供表单和业务回调。

该 probe 在默认测试中明确标为 ignored 的上游接收门禁；显式运行在当前 pin 下预期失败，不能计为功能通过：

```sh
CARGO_TARGET_DIR=/tmp/agentkit-layout-text-292-target \
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 \
cargo +1.95.0 test --manifest-path ui/Cargo.toml --locked --offline \
  -p tekes-agent-ui-consumer-check --test summary_label \
  original_popover_content_child_wins_over_background_sibling -- --ignored --nocapture
```

收到固定公开入口后，应迁移 probe/实际表单并取消 ignored，验证整个原 Content 组合；不能仅删去红色门禁或保持临时内联而宣称完成源 View。当前保留可运行的内联表单，表单内部的改动不替代最终锚定 Popover。占位提示、完整错误显示语义、弹层按键/关闭、配对视觉等仍需随实际源 View 完整验收。

## 验证范围

- UI workspace496项通过，新增1项上述明确 ignored 的上游门禁；反馈宿主22项通过。第二 consumer 在所有主题、0.85/1/1.5字号和240/400/900宽度下检查原编辑视口、靠右动作、Primary、AX 名称、原 note owner 与主 Composer 组字 baseline。保存中继续编辑、一次请求和关闭后失败在实际 binding 测试覆盖。
- strict all-target Clippy、fmt、原生构建通过。首次导入路径和 probe lint 错误修正后重跑通过，失败日志保留。
- 最终冻结程序 `.runtime-fixtures/AgentKitFeedbackFormFinal.app`，SHA256 `faa58ba6cef1a8fd1c46cd18c69e4535006a305baa08c7af451d944715de2f28`：真实 Pi1.3.0、DSH0.2.1-rc.2、Native Kernel0.9.3 原生评价/中文备注保存、重开、删除、会话切换与草稿/主编辑器保持、IME send/navigation 屏障和零 prompt 提交均通过。provider 是隔离本地受控服务；Native 明确 UI-only，不证明生命周期/archive/child。
- 同一最终程序的系统简体拼音通过，前后输入源均 `com.apple.inputmethod.SCIM.ITABC`。原生反馈组字使用合成文本服务；software Pinyin 不代表实体键盘、候选窗、VoiceOver、完整视觉或阅读锚点通过。
- 首个程序 `b7c98b6e…` 的 Pi 原生通过；DSH 前台切回 Codex后焦点断言失败，Native未执行，原日志保留。发现该探针启动前没有窗口 active 前置条件，已补与原阅读探针一致的检查及 active 日志；没有请求激活、重试输入或放宽后续焦点断言。最终三端是新程序的独立完整运行，不改判旧程序失败。

这是同进程原生证据，不是 UI 跨进程证据。AgentKit 最新 release 核对仍为 v0.3.8；UI仍为0.0.0/publish=false。[ProcessContainer #48](https://github.com/TekesApps/TekesUI/issues/48)与完整目标继续未完成，PR #10保持草稿，不因此合并或打tag。此前 Native 流式阅读的未完成项也不由本次反馈专项替代。

详细收据、来源哈希与范围见[同名 JSON](rust-agent-ui-feedback-form-20261022.json)。证据在 `.runtime-fixtures/feedback-popover-20261022/`。复用既有 target，未新增独立构建缓存；保留两份验收程序、失败、源快照、日志、收据和图像。没有删除用途未明的共享缓存，释放空间0。
