# Sidebar：首次恢复与观察失败

本轮对照当前 ClientKit `Sources/Composer/Agent/ClientAgentSidebarListView.swift` 的 `observe()`：首次 outline 恢复前隐藏行并显示 loading；恢复后才允许 empty；已恢复后的连接变化不重新宣告未恢复。Tekes `Tekes/ClientAgentSidebarHostAdapter.swift` 只提供产品配置和宿主能力，没有分叉此状态机。本轮仅修改 Rust AgentKit。

## 公开输入与显示

`SidebarInput` 增加 `loaded: bool` 与 `error: Option<String>`，`SidebarLabels` 增加 `restoring`。`Sidebar::displayed_input()` 返回完整的已接受输入。UI 包仍为实验 API，尚未发布。

- `loaded=false`：显示恢复提示，不显示输入中可能缓存的行、选中项及共享动作。不能将它当作确认空列表。初次构造也显示恢复提示。
- `loaded=true`：按输入顺序显示会话；只有此时零行才显示 empty。
- `error` 是独立观察失败提示。已加载的行、原控件及选择保持，活动事实可以标为 stale；离线可读/复制与 mutation 的能力判断仍走现有 shared definitions 和 dispatcher。没有隐式重试或提交。
- 同一 inventory publication 的 rows/loaded/error 不可改变；同 revision 的冲突源值被拒绝。选择由宿主独立提供，可以在同一 inventory revision 下变化，不要求伪造库存更新。隐藏/移除后重现的行分配新的单调 ID，旧手势不能命中新行。

`SidebarPublication::input()` 现在传递两个状态。`ComponentDemo` 的同步比较也包含它们，不再漏掉仅有恢复/失败变化的发布，并拒绝同 revision 不同源值。

共享 `target=state` inventory 帧增加 `inventoryReady`，取自**同一次读取的 MemoryProjectionStore**。binding 在这个事实到达后才标记初次恢复完成；不能用“收到 state 帧”代替 inventory baseline。此 loaded 标记对同一个 binding 保持，重连不会清除已恢复的列表。binding 重建则重新等待自己的首个 baseline。该字段是 AgentKit 服务的附加读取事实，不修改 SEP、Kernel 或 harness 协议。

基础文字和滚动继续使用 TekesUI 默认组件及语义间距；没有消费侧颜色、字体、圆角皮肤。当前显示是恢复/失败文字，**尚不是完整 ClientKit status-pane/outline 排版**。分组、搜索 footer 与 Retry、重命名完整布局和最终配对视觉验收仍需继续完成，不把本轮记为 Sidebar 全部对齐。

## 验证范围

组件测试覆盖恢复≠空、非空缓存不可操作、失败保留原 widget/目标、恢复状态切换淘汰旧 ID、同发布冲突。第二公开 API consumer 覆盖 publication → input → mounted component 的恢复/错误值。

延迟 baseline 的集成测试在真正的 SharedAgent/SessionRuntime 中阻塞 inventory 流，验证已收到初始 state 帧仍未恢复；放行后恢复；再次阻塞重连 baseline 时保留同一行身份，不创建副本或重试 mutation。

新增 `--components --transcript-content --sidebar-restore-selftest` 原生窗口探针，只允许 owned fixture，依次渲染恢复、非空、注入失败、重新未恢复、确认空及再次非空。它验证真实组件树和 AX 发布中的提示、控件保留及 ID 淘汰；**不证明真实 harness 断网、物理输入法、VoiceOver、系统动量或完整 ClientKit 视觉一致性**。原生 probe 的错误是明确的 UI 发布夹具，未伪装为真实 transport 错误。

本轮收据见同名 JSON；未重跑或升级冻结 TekesUI 候选。默认依赖仍精确固定公开 `8cf0840790334ba224afc0595897c74231c712e6`。
