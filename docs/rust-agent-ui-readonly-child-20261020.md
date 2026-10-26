# 只读子历史消息树与读取回调（2026-10-20）

`ReadOnlyTranscript` 组合现有 TranscriptContent、TranscriptPaging 和 TekesUI 的默认 viewport/按钮/状态文字。Native 有界子历史现在显示实际消息、推理、工具来源详情及 footnote，不再只显示数量。它不构造 RunController / RunSnapshot / Composer，不返回 Send、Cancel、问答回应或 turn mutation；即使输入带有 enabled 的 footnote 定义，该 lane 仍禁用。

AgentKit 基线仍为已发布 v0.3.8，UI 包未发布；TekesUI 固定公开 `8cf0840790334ba224afc0595897c74231c712e6`，八包199输入 SHA `f0af230a1efe97334b90eb193f77ca0d30dec43ef09e487ad6948c38a938257f`。没有修改 Kernel/TekesUI，也没有 Swift 源码或构建步骤。只读对照 ClientKit `c289bc9d33edcd83a782ac17927a8c65a027eccd` 的 RunChildHistoryObserver 和 ClientEmbeddedRunView，及 Tekes `8858ccd9d46590852ec7a0949aee289fc6798d70` 的 host/panel adapters：来源由完整 scope 与挂载生命周期决定，不能回落父会话历史。

## 输入与所有权

```rust,ignore
let mut body = ReadOnlyTranscript::standard(100, 103..1103, source.binding().scope().clone(), Default::default())?;
let frame = source.snapshot().await?;
body.update(source.readonly_input(&frame, reading, error.as_deref())?)?;
```

standard 使用三个连续 namespace（控件、分页、内容根），消息行池由宿主独占预留。也可用 new 明确提供 IDs 和同 scope 的 TranscriptContent。节点不循环复用，支持原生只读选择/复制及共享局部展开。用户内容仍为右侧气泡/外置 caption，机器正文、推理、工具与 aside 靠左；字体、颜色、圆角、间距和控件样式继承 TekesUI。

`PaneSnapshot::ReadOnlyChild` 增加 `publication` 和 `connection_epoch`。这属于尚未发布的 UI binding API，不是 SEP wire 字段。publication 是原 binding 的 snapshot **调用顺序**：在 await 前分配，不按完成顺序计数，也不是 journal sequence、throughSequence、historyRevision 或阅读锚点。旧结果不能覆盖更晚已采用的 frame。

读取跨过 transport epoch 会失败；读取期间来源退休则返回 Missing。connection 与 epoch 在返回时配对检查，非 fresh 连接会把历史标为 stale。组件保持原 scope，并拒绝旧 publication/epoch、同版连接或数据冲突以及非法内容；失败时保留已加载控件。关闭/换源后，宿主继续用原 ChildPaneBinding Arc 和 tab token 拒绝晚回调，不根据同名 session 重新选择接收方。

reading / read_error 是原读取任务的显示输入，组件不建立第二个请求调度器。ChildRun/BoundedHistory 继续负责公开 API 读取、frozen cut、prefix 校验与页合并；自定义 ChildHistorySource 的分页能力保持 false，不能因为 DTO 有 has_more 就调用 Kernel 分页。

## 事件回调

每次 UI 事件后 drain_actions，调用 take_intents，取得不可变 ReadOnlyTranscriptIntent。它只表示 Refresh 或 Earlier，携带原 scope/publication/epoch。将其交给**原 source** 的 `read_intent(intent)`，不要在执行时查询当前选中的 pane。

- 每次手势共享一次性 admission；克隆的句柄不能再次派发。失败、取消或丢弃后不自动重试。
- Earlier 在 IO 前再检查 scope、来源存活、公开分页能力与 epoch；旧页意图不能在新连接中自动获得权限。
- Refresh 是显式重新读取，允许用户从旧 frame 更新到已就绪的新连接；旧 frame 的加载更早按钮保持禁用。
- 错误回到原 pane，保留旧消息和选区；重试需要新手势。读取/重连通知按已有 refresh key 合并，不按帧发起网络读取。

宿主拥有执行器和窗口级焦点屏障。父 Composer 或其它 editor 正在组字时，继续先拦截会改变焦点的 pointer/AX；不要把只读 lane 当成结束其它编辑器组字的许可。文字服务、焦点/选区、IME 和通用滚动仍由 TekesUI 提供。

## 验证与限制

结构化收据见[相邻 JSON](rust-agent-ui-readonly-child-20261020.json)。UI workspace 236 项（含 7 项新增只读用例）、脚本 18 项测试、严格 UI Clippy/fmt、八包输入与依赖边界检查通过；headless 行为未改，未重复全量 root tests。新增用例验证晚完成的旧读取、跨 epoch 读取、来源退休、分页后的实际消息节点/选区保留、错误保留、同版/非法 frame 拒绝、disabled mutation，以及旧页拒绝/显式刷新/克隆 admission。第二 consumer 通过公开包单独构造、同步只读历史。

原生 fixture 检查分页扩展的是实际渲染消息，而非计数。真实 Native task child 通过公开 bounded API 到消息树和 GPU，展开只读 source-call 详情、捕获原来源读取、重连并保留控件，且无 Composer/不允许 Send/不改变父草稿。小型真实 child 没有更早页时，不把 refresh 当成分页通过；多页扩展另由 fixture 的窗口与组件测试证明。Pi/DSH 按各自独立 child/session 能力保持原 RunView 路径。

ScrollContent 是通用 viewport；保留节点/选区不等于分页保持阅读锚点。[TekesUI #22](https://github.com/TekesApps/TekesUI/issues/22) 的混合阅读事务和 [#25](https://github.com/TekesApps/TekesUI/issues/25) 的可选中富内容仍待消费。Markdown 仍为源码，附件仍为原始信息；物理中文 IME、完整三端原生 UX、其余 footnote 能力和完整 Run/Transcript 验收未完成。
