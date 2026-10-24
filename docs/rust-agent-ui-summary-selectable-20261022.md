# Summary 静态行的原生可选择文本

只读对照当前 ClientKit `ClientAgentSummaryView.SummaryDefaultRowView.configure/hitTest`：有动作的行使用原按钮，无动作的行将 title 设为 selectable，输入命中交给原 label。Tekes `ClientAgentSummaryHostAdapter` 只提供仓库/导航能力，没有另一份文本选择实现。

AgentKit 原来将静态行与用量标题都渲染为不可选择的 `TextContent`。现在无动作的普通行和用量标题使用原 TekesUI `TextFieldContent::read_only_content`：没有表单装饰，保留默认UiBody样式及库自己的文本服务、选区、只读写入拒绝和复制路径。用量右侧metadata仍是UiCaption `TextContent`，与源View只选择主label的规则一致。有动作的行继续使用原业务按钮，未知/空态与章节顺序不变。

更新通过原控件的 `apply_external_value`，带稳定行ID、Summary publication和原baseline，并使用SDK的Preserve selection/undo；没有直接替换input文本或复制选择/剪贴板状态机。普通更新、章节折叠与再展开保留原文本owner，切换scope/行身份则退休旧owner。现有Summary输入校验仍先拒绝陈旧、冲突和跨scope事实。

## 公开宿主接入

`SummaryView::passive_text_id(section, row)` 查询当前静态行的原文本ID；usage行返回title槽。它是身份查询，折叠时该ID仍可存在，但inactive子树不发布可访问性/输入槽，不能把ID存在当成可见性或动作许可。激活仍按原`SummaryIntent`处理；静态文字选择不产生OpenTask/OpenArtifact等业务动作。

宿主继续把Summary挂在同一个ContentHost，并提供SDK的TextSurfaceHost；使用已有文本服务与命令路由，不需要额外Copy按钮或自己的选区store。第二公开consumer包含独立静态Summary的选择用例，不依赖runtime/harness。旧代码若依赖内部`TextContent`downcast需改用公开ID与原`TextFieldContent`，不能改变其read-only或复用到可写Composer。

## 验证与范围

447项UI workspace测试通过，包含第二consumer和原CoreText用量布局矩阵；strict all-target Clippy、fmt、边界检查、native build通过。新增测试覆盖选中文字、ReplaceSelectedText拒绝、折叠期间更新保留原owner/选区、隐藏语义树、scope改变后旧owner消失及无业务回调。用量原格式、数值右对齐与长标题布局继续通过。

冻结最终原生程序 SHA256 `a2226b2733055835c1528c8b112cc5036c0686a3face2fb6cabdbdc4aacb6b96`，保存于 `.runtime-fixtures/AgentKitSummarySelectableAccepted.app`。`--components --transcript-content --summary-sections-selftest`通过原章节点击折叠/展开、真实control projection用量、原native文字/AX选择、写入与预编辑拒绝；随后原binding发布新草稿，Summary文字owner/focus/选区保持，提交数不变。最终截图已检查“输出”选区、左右用量槽及独立Composer草稿。

最初程序的旧探针只接受ButtonContent/TextContent，遇到正式只读行即失败；类型查询适配后，原生文字/AX断言通过，但空闲窗口未唤醒，停在最后publication检查前按deadline失败。两次失败日志/程序分别保留，未计通过。最终只补类型适配和测试专用WakeRedraw，不放宽原断言；447测试对应最终业务改动，后续探针改动另以最终Clippy/build/native收据验证。

证据在 `.runtime-fixtures/summary-selectable-20261022/`：workspace/Clippy/build日志、失败和最终程序身份、原生日志、截图与源码/reference哈希。系统剪贴板未被操作，收据明确`systemClipboardExercised=false`；不是实体键盘、候选窗、VoiceOver或完整ClientKit配对视觉验收。上一阶段真实三端证据属于其各自冻结程序，本次不冒充重新验收三端。

本阶段只消费精确公开TekesUI `09b4bab44b6dfa65487250ea816b610b9df59095`，没有修改SDK/Kernel/ClientKit/Tekes。未发现需要新建的基础能力缺口。完整Summary默认行icon/status/trailing/indent、自定义host行、header/chrome及配对视觉仍待补齐；原有#25/#38/#45等上游范围不因本次文字选择而关闭。

复用 `/tmp/agentkit-layout-text-292-target`，debug0、incremental关闭，无新target。源码/WIP、固定依赖、失败记录、截图、收据与验收程序保留；本阶段未删除用途不明确的共享缓存，释放字节0。PR #10保持实验草稿，整体交付尚未完成。
