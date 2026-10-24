# 公开 Transcript 内容树（2026-10-20）

`TranscriptContent` 是可独立挂载的真实内容组件，组合 `InputMessage`、`OutputMessage`、独立 `ToolActivity` 与 `TurnFootnote`。它消费现有 `TranscriptSnapshot`，不重新构造 Run 状态机，也不是完整 Transcript viewport 或 RunView。`RunSurface` / `ComponentDemo` 继续仅作测试宿主。

## 接入与所有权

```rust,ignore
let mut content = TranscriptContent::new(
    700, 701..1700, original_scope.clone(), Default::default(),
)?;
content.update(
    &original_scope, run.revision, run.connection_epoch, &run.transcript, available,
)?;
```

宿主授予互斥 ComponentId namespace 池，每行独占一个 namespace，移除后不循环复用。固定 original scope；revision 与 connection epoch 必须来自同一 RunSnapshot。组件保留最后的渲染输入供同版冲突校验，不作为第二会话存储。宿主转交原 ContentHost action batch 给 `handle_actions`，取得的 TurnFootnoteIntent 仍交原 RunBinding 的 TurnActionDispatcher；本地展开无需 endpoint 调用。

宿主将其套入 TekesUI ScrollContent，并以公开 StackContent 的 flexible_child 约束剩余高度。实验宿主为 Main 和各 pane 分配独立 namespace 池；切换会话保留控件和各自草稿。窗口/整体布局归宿主，读取锚点、通用文本服务和滚动引擎归 TekesUI。

## 排版与样式

- 用户行：附件/原始信息详情 → 正文圆角气泡 → 外置次级 footnote，整体靠右；短正文自然收窄，纯附件不挂空正文文本目标。
- 机器行：正文、推理、工具及 turn aside 靠左；工具结果更新已有卡片，推理/详情使用共享局部展开状态。
- 颜色、字体、圆角、控件样式和语义间距来自 TekesUI 默认组件/tokens/宿主主题。AgentKit 不维护覆盖表；明确升级不可变 TekesUI pin 后，基础样式变化随组件统一体现。

只读对照：Tekes `8858ccd9d46590852ec7a0949aee289fc6798d70`，ClientKit `c289bc9d33edcd83a782ac17927a8c65a027eccd`，MetalTextKit `7ee9703fd28c3cb7a65ab6b298c78efb17dd4507`。MetalNativeTurnView 的 inputAttachments/inputBubble/inputAside 给出以上用户顺序和 trailing 约束；TranscriptTurnFootnote 区分 input/output lane。不修改或构建 Swift。

## 身份、顺序与失效

NativeEntry 是公开持久身份；Item 仍是 legacy/provisional 身份，不升级为持久阅读锚点。仅显式 reconciliation receipt 允许 Submission → NativeEntry 复用控件；不匹配正文或时间戳。多条记录同时认领一个已挂载 receipt、重复行、旧来源/epoch/publication、同版事实冲突或 namespace 耗尽均在树修改前拒绝。

内容顺序来自 TranscriptDocument.event_order；工具优先放在已知调用位置，仅结果位置已知时放在结果位置，完全未知时明确显示覆盖提示。消息内已证明属于该块的工具引用不再重复成独立卡片。会话提示没有伪造的 turn footer；未定位预览不强行关联持久输出。

保留未移除消息的实际文字控件、选择与本地展开状态。被裁出窗口的 footnote 仅在操作仍未确定时保留，禁用并退休旧按钮；Unknown 不因剪裁或新 epoch 自动重试。其它移除内容不建立第二历史缓存。

用户附件按出现次数与 canonical/aggregate 镜像对账；保留两份相同附件。未知内容保留为原始详情，不执行工具或静默丢弃。附件不下载、不解码，不冒充图片/富文本预览。

## 验证与边界

结构化收据见[相邻 JSON](rust-agent-ui-transcript-tree-20261020.json)。当前 UI workspace 221 项、脚本 18 项测试通过；UI 严格 Clippy、fmt、实际八包199输入及边界校验通过。headless 行为未改，本轮未重复全量 root tests。组件和第二公共 consumer 验证身份迁移、Native 前插、源顺序/工具合并、选择保留、非法输入原子拒绝、Unknown action 和 namespace 失效。浅深色 CoreText 几何验证用户详情/气泡/footnote 的右边缘与纯附件可见性，实验宿主另检查 Composer 保留在窗口内。

真实 Native/Pi/DSH 使用隔离会话及本地合成 provider，验证长历史分页/重连与附件后经 canonical snapshot 到混合内容树的 GPU 路径；不调用付费模型。原生 fixture 窗口验证独立草稿、发送/取消、重连、任务/子代理/SideChat 和内容树 footer 的实际指针入口。自动文本注入属于合成文本服务，不证明物理中文输入法。

能力显式为 mixed_content=true、structured_user_content=true、keyed_reading_transactions=false、rich_content=false。[TekesUI #22](https://github.com/TekesApps/TekesUI/issues/22) 与 [#25](https://github.com/TekesApps/TekesUI/issues/25) 仍开放。分页前插保留控件身份不等于保持阅读锚点；现有 ScrollContent 不提供此验收。Markdown 仍为源码正文，图片/富内容、匿名 preview 到 durable 的可靠阅读关联、物理 IME、完整三端原生 UX 和正式 RunView/Transcript 尚未完成。
