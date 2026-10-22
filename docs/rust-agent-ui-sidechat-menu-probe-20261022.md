# SideChat 家族菜单原生探针（待运行）

当前 ClientKit `SidePanelTab.sideChat` 和 Tekes `MainWindowToolbar.fill` 契约为：
一个 SideChat 家族入口；新建项在前，同名成员跟随，其当前项勾选；宿主释放临时会话成功后移除标签。
此增量不改变业务组件、绑定或基础控件，只扩展已有受控 SEP 原生探针。

新增 `--panel-side-chat-menu-selftest`，必须同时指定 `--components --transcript-content
--panel-navigation-toolbar`。它复用原 panel-navigation fixture 与收据通道，和其他自测模式互斥。
完整运行参数为：

```text
--components --transcript-content --panel-navigation-toolbar --panel-side-chat-menu-selftest
```

探针通过原 launcher 创建第一个 scratch，经原 SDK 菜单的新建项创建第二个，再通过两个同名项切换；
核验当前成员勾选和第一个 editor 的实例保持，逐个关闭，直到 catalog 与面板均为空。
每次点击使用 SDK 发布的语义部件坐标和原本地 AppKit 自测事件，不轮询菜单最后选择来生成业务意图，
也不复制菜单、命中、焦点或弹出层实现。沿用 25 秒原期限，不重试激活、不放松断言。
终态为 phase 25，原 `NATIVE_AGENT_PANEL_NAVIGATION_RECEIPT` 另记 `originalPointerMenu` 等字段。
旧 `--panel-navigation-selftest` 的 phase 14 收据与行为保留。

本轮宿主回归 14 项、第二公开 consumer 3 项、严格 Clippy 和 demo 编译通过。
这些仅证明已有组件/宿主回归与新增探针可编译，不能证明新增原生路径运行正确。
原生窗口及输入尚未启动：桌面空闲协调未收到答复。不可将 ac51636 的原生结果转移到此新程序。
运行时应另保留待验程序和证据，不覆盖既有已验收程序；当前没有新增程序副本或 target。

精确公开 TekesUI 仍为 `50ec49f3446924fdbc66b3cff22cd8cfe150fe7c`。
AgentKit 最新 release 仍为 v0.3.8；UI 继续显式消费仓库内开发路径，未发布新的共享契约。
TekesUI ProcessContainer #48 仍 OPEN，当前探针仅验证同进程受控 SEP，不代表跨进程、真实三端或窗口级 toolbar。

[检查记录](rust-agent-ui-sidechat-menu-probe-20261022.json) 保存来源和日志哈希。
工作仍在 PR #10，未合并或打 tag。本轮没有删除缓存或证据，释放空间 0。
