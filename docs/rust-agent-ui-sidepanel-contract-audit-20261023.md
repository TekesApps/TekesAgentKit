# SidePanel 源码契约核对（2026-10-23）

本轮核对 ClientKit checkout `c289bc9d33edcd83a782ac17927a8c65a027eccd` 的
`ClientAgentTaskView`、`TaskRunPanelProjectionState`、`ClientSideChatRunView`，
以及 Tekes 的 `SidePanelHostView`。只读参考两个仓库，未添加 Swift 或修改宿主。
这是源码和现有组件契约核对，不是最终同内容布局或原生窗口验收。

| 场景 | ClientKit 行为 | AgentKit 当前承接 |
| --- | --- | --- |
| Task tab 身份 | 保持 task ID，按最新任务行解析 session；更换指针时替换 Run | `SidePanelTabKind::Task`、`TaskPaneBinding` 和原 `TaskPanelState`；scope 是内容数据，不是 tab 身份 |
| Task 加载/缺失/错误 | 尚未解析显示 loading，已解析无值显示 unavailable，不借父 Run | 复用 resolved/value/error；没有值时 `open_resolved` 返回 None，SidePanel 显示对应占位 |
| Task 迟到结果 | task ID/取消状态核对后才采纳 | 原 load token、source generation 和 `TaskRunGuard`；包括 A→B→A、同任务重新加载和旧 baseline 回包 |
| Task 关闭 | 本地 tab 导航，不取消远端任务 | `SidePanelIntent::Close` 由宿主导航采纳，不派生 cancel/discard |
| 子代理有界历史 | 保持原直接 parent 与来源边界 | `ChildPaneBinding` + 原 child catalog/`ChildHistorySource`；SameSession 是只读内容，无 Composer，不显示父正文代替缺失子历史 |
| 子代理控制权 | 控制能力与可读历史分别处理 | refresh 复用原 catalog；失效/错误收回控制，保留可读历史，不按布局或缓存存在推断可写 |
| SideChat 内容范围 | 使用临时 fork 自己的线程/主 Run，不使用 panel 父线程 | `BoundSideChatRun`/`SideChatBinding` 提供来源角色证明；控制仍来自原 controller |
| SideChat 尚未就绪/已消失 | 缺少原线程时不呈现一个新对话 draft | 先等待公开 history baseline，再复核原 pointer；权威 absence 通过现有 `reconcile` 收回来源，不回退到 main cache 或父 Run |
| SideChat 关闭/结果未知 | 释放成功才关闭 tab，失败保留 fork | 原 Ready/Closing/Unknown/Missing 门禁；不自动重试未知 mutation，不清理未确认的 draft |
| toolbar 与整体布局 | Tekes host 安装 shared tabs、select/close/newSideChat 回调 | `SidePanelNavigation` 消费同一 `SidePanelInput` 并返回原 intent；宿主拥有窗口、toolbar 分配和导航采纳 |

特别保留的边界：临时 SideChat 可以不进入普通会话 inventory，不能把列表缺席当作删除，
也不能据此清理草稿。原 `SideChatBinding::reconcile` 需要权威 pointer 查询结果；
查询失败不等于 absence。这里没有新增“观察 inventory 自动关闭”的第二套状态机。

相关回归位于 `task_binding.rs`、`sidechat_binding.rs`、`child_binding.rs`、
`side_panel_component.rs`、`panel_navigation.rs` 和 `side_panel_navigation.rs`。
本轮没有发现需要重写上述业务状态的差异；完整窗口 chrome、焦点/IME 的当前原生组合、
ProcessContainer 同/跨进程仍是未完成项，不能从这份核对推导完整 Views 对齐。

本轮额外修复了验收工具链的重复升级归档冲突，见[固定宿主说明](rust-agent-ui-native-toolchain.md)。
Pi 1.4.0 兼容交接独立保留在 [Pi 兼容证据](pi-1.4.0-compat-20261023.md)，
不覆盖此前六份 Pi 1.3/DSH/Native 离屏程序的来源身份。
