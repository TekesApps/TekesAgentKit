# Composer 显示规则：归档、中断与暂不可提交

正式 Composer / RunView 对照 ClientKit `ThreadStatusPolicy.acceptsInput` 与 `RunView.composerIsPresented`：`Archived` 隐藏 Composer；`Interrupted`、`Unavailable`和暂时关闭发送权限都保留已有输入区域。`can_submit` 和 `ComposerControlGate` 只收紧操作权限，不充当显示状态。主 Run 以及复用 RunView 的可写任务、独立子代理和 SideChat 共用组件策略；Native 有界只读历史仍用 ReadOnlyTranscript，不凭空增加 Composer。新会话/workspace 能否挂载由宿主输入决定。

新增公开查询：

- `Composer::is_presented()` 返回当前 canonical frame 派生的可见性。归档时拒绝旧 Send/Cancel/入口意图，退出组字导航屏障；不把失联视为归档。
- `RunView::displayed_snapshot()` 返回实际安装到该控件树的 publication，供宿主将状态与当前几何/owner 配对。运行时在绘制后可能已有更新；不要把再次读取 binding 的新数据配到旧控件几何上。
- 第二公开 consumer 的 `composer_focus_target` 展示宿主如何查询共享可见性再发起焦点请求，不复制 status 策略。

Composer 保留原 Content / TextSurface。TekesUI 的 `Component::active` 拒绝绘制、指针/键盘与 AX 路由；ContentHost 自行 reconcile 已失效的焦点并通过现有 blur 完成 marked text。AgentKit 不提交/取消组字、不改写撤销/选区或平台文本服务。隐藏时只在 Run 的布局阶段省略该业务 slot，继续用公开 StackContent 测量/排版；不残留隐藏区域或多余间距，挂载树仍保留原编辑器与命名空间。

组件测试覆盖 interrupted、unavailable、gate closed、archived、恢复，检查可见性、原实例、草稿/行身份、隐藏 AX/焦点和旧 Send action 拒绝。CoreText 正式 Run 测试在固定输入下检查释放全部 Composer 高度及间距，并验证库负责 blur/unmark、没有隐式 Send/Edit、恢复不抢焦点。

`--components --transcript-content --composer-visibility-selftest` 是新的本地原生窗口探针：通过正常 runtime 调用 fixture 的 archive/unarchive，走 GPU RunView、平台合成 marked-text、公开 history lease 读取恢复，核对原编辑器、本地中文文本、AX/焦点、释放的底边、B 草稿及无提交。归档会关闭 journal；取消归档只改状态。重新打开 pane 需要公开 history lease 取得 baseline；探针临时获取再释放 owner，同时保留原 binding 的 lease，不创建第二个 Run 状态机。

原生探针首次错误地把阅读区总高度变化全算成 Composer 的释放量；实际归档同时增加20点分页状态行。现检查释放的**下边界**，固定输入的组件测试仍检查完整高度差。一次原生探针又把绘制后新到的 binding publication 与旧控件树配对，促成 `displayed_snapshot` 查询。未重开 journal 的取消归档测试则停在不可提交状态，按公开读生命周期补全后通过。所有初始失败日志保留，未计为通过。

精确门禁、文件哈希和分层证据见[JSON收据](rust-agent-ui-composer-visibility-20261021.json)。该探针是本地 SEP fixture 与合成原生文本服务，不是实际三端/实体 IME/VoiceOver 或完整布局验收。默认公开 TekesUI pin 不变；完整文档/Composer/chrome/selection 叠层、rail、阅读集成及其它 pane 的显示审查仍按[原清单](clientkit-view-layout-review-20261020.md)继续。RunSurface仍只作测试宿主。

待继续修正的降级边界：`crates/presentation/src/controller.rs::Shared.refresh` 在历史投影失败时把旧 Run phase 一律改为 Unavailable，可能令已知 Archived 丢失。需要控制器回归与修正，不能把本轮正常 canonical phase / fixture 生命周期专项扩大为该错误路径已验收。
