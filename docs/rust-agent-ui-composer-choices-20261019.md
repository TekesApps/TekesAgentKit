# Composer 模型/预设内容增量

本增量提供可供宿主挂载的 `ComposerChoices` 和连接既有 presentation 的
`ComposerSettingsRequest`，没有完成模型/预设原生入口或真实目录适配器。
AgentKit 仍基于已发布 v0.3.8；TekesUI 固定公开
`431093786de47b48cd566d0d9c0eafa088b10d68`，188 输入未变。

只读对照 ClientKit 的 ComposerModelSelectionCommand、ComposerModelPanel、
ComposerPresetCatalogRead 与 Tekes 的 ClientAgentHostAdapters：接收目标与目录
结果必须保留作用域，目录读取/选项交互不能把结果写进后来选中的会话。
没有修改 Swift、TekesUI、Kernel 或 presentation 状态机。

组件输入明确区分 model/preset，使用 destination、局部请求代数、单调 catalog
revision 与 choice ID；保留 selected/disabled reason/loading/error，未知当前值
可见但不会自动选择第一项。它组合公开 Stack/Text/Button，由宿主提供通用容器与
滚动/弹层能力。按钮 pending 只阻止重复点击；旧目录、旧控件 ID 与迟到完成不会
命中新打开的目标。不实现菜单键盘导航、焦点引擎、窗口或编辑器。

绑定返回保留原 Arc<RunBinding> 的一次性 request。提交委托原 RunController 的
selection-generation 检查和 commit_settings，保留文字/附件/批注，并重新检查
task/child/SideChat 的指针生命周期。发送继续由既有 PreparedSubmission 冻结设置，
宿主 SubmissionPreparation 负责 model/preset/auth 的真实操作，不从 prompt 推断。
宿主仍需检查当前导航与目录能力，取消时释放尚未提交的 request；保留旧 pane
不等于它知道当前窗口导航。完整责任见 [公开接入说明](../ui/bindings/README.md#composer-model-and-preset-content)。

本地 UI workspace 92 tests、strict all-targets Clippy、fmt、demo build、依赖边界及
实际 Cargo 解析输入检查通过。新增 3 项测试覆盖目录 A→B→A/版本、未知当前值、
禁用与失败、pending、关闭、过期 AX/完成、共享 settings generation、草稿隔离、
Send 设置冻结、子代理撤权及有界只读拒绝。第二公开 API consumer 增加独立挂载，
仍只依赖两个公开 UI 包；所需 destination/scope 类型由 components 重新导出。

尚未验证：demo 中的目录读取/模型预设操作、实际原生面板、三端模型/预设入口、
provider-specific reasoning/fast/thinking 控件、附件/批注卡片和物理 IME。
demo 默认 NoSettings 仍拒绝非空 settings，没有伪造配置成功。上个增量原生和真实
三端收据不扩充为本组件的原生验收。PR #10 保持 draft，后续继续挂载和验证。
