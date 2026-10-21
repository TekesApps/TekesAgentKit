# Sidebar 共享会话菜单与置顶移动

当前精确公开 TekesUI 仍为 `09b4bab44b6dfa65487250ea816b610b9df59095`。本轮对照当前 ClientKit `ClientAgentSidebarModel` 的会话菜单目录及 `ClientSessionSidebar.movePin`，并只读检查 Tekes `ClientAgentHostAdapters.swift` / `AppShell/MainWindowToolbar.swift`；未修改这些仓库或 Kernel。

## 公开组件与回调

`SidebarSessionMenu` 是业务组件，内部只声明原 `MenuContent`。共享目录、enabled/disabled reason、copy payload、interaction 及 endpoint/session identity 原样来自 `ThreadActionsPresentation`；UI 不另建动作策略。置顶行在归档/取消归档之前补上 Move Pin Up/Down，计划仍由原 `Sidebar::move_pin_change` 生成。两端的命令和 ClientKit 一样可显示，超出顺序边界时没有写入或 RPC；没有偏好写权限时禁用移动。

默认 `Sidebar::new` 已使用紧凑菜单；`with_session_menu()` 可显式声明同一模式。`with_action_buttons()` 仅保留旧按钮面板的兼容/诊断路径，不是本轮 ClientKit 对齐的默认显示。其旧 `action_id` 只在按钮模式返回真实已挂载目标，菜单模式返回 None，不提供假的隐藏按钮 ID。已有按钮回归显式标明兼容范围。

菜单模式从 `session_menu_id()` 取得组件，`SidebarSessionMenu::menu_id()` 只在真实已恢复的选择源存在时返回原 SDK menu ID；宿主通过原语义子项/输入操作控件，不反算行坐标。独立组件也可用 `update_from_sidebar(&Sidebar)` 读取同一已接受的业务事实。

宿主在事件边界只 drain 一次 `ContentHost::drain_action_records()`，用 `Sidebar::take_menu_intents` 取得 `SidebarSessionMenuIntent`：Shared 返回原 `SidebarIntent`，Preferences 返回捕获的原 CAS change。与其它控件共用这些记录导出的旧 action 列表，不再单独 drain 一次。选择、动作目录、pending、pin 状态和移动计划变化会替换原 Content mount / SDK model；旧记录不能改绑到新的当前选择。单纯活动/标题/路径刷新不重建菜单，仍已打开的原 popup 保持。

demo 已改为消费这些回调，继续使用同一共享 dispatcher、重命名 pane、分叉/归档导航、clipboard adapter、pending admission 和偏好存储。没有乐观更新 pin 顺序，不在 UI 再建 pin 数据库，也不自动重基线/重试。第二公开 consumer 已接相同 rich-record API。

## 验证

- UI workspace **440 项**、strict all-target Clippy、fmt、静态边界通过。新增菜单专项涵盖原 pin 边界和 CAS 计划、同名 session 的 endpoint 隔离、捕获意图保留源而旧记录退休、pending/只读/恢复前的禁用、无关活动刷新保持原模型及已打开 popup。第一版专项发现保留 ID 与旧 footer/分区冲突，已改为原 namespace 的空闲12/13槽，失败日志保留。
- 原 demo 的 rename→fork→archive→恢复选择测试已通过实际原菜单及 CoreText 布局，未改回读取最终 selected；重复消费原 popup generation 不再次发 fork。旧按钮兼容用例另记，不把它们当新菜单验收。
- 同一冻结程序的原生 `--sidebar-session-menu-selftest`：先观察到实际菜单位置的 pointer-up，再验证中文组字屏障、原 focus/opaque baseline；Unmark 后通过原鼠标菜单动作分别置顶A/B，再下移A，原存储确认 `[b,a]`，原行实例和A/B草稿保持、无提交。截图已检查，窗口exit0。
- 同程序 `--thread-actions-selftest` 原生通过 rename IME barrier、共享库存回传、fork导航、archive清空和恢复选择、草稿隔离。旧脚本不再寻找按钮；它只捕获原菜单模型/标签，在成功布局的原语义子项上发送鼠标动作，迟到模型变化直接报错，不自动重绑或使用已知有缺口的空字符控制键注入。

证据保留在 `.runtime-fixtures/sidebar-session-menu-20261022/`，冻结程序 `.runtime-fixtures/AgentKitSessionMenuCheck.app`；日志、原截图和源码哈希分别记录。fixture 是本地受控端点，工具合成 AppKit 指针/preedit，不升级为 Native/Pi/DSH 三端完整原生回归、实体IME/候选窗、VoiceOver或ClientKit配对视觉验收。完整阅读探针仍沿用前轮未通过状态，本轮没有把菜单通过当阅读通过。

## 仍需完成

本轮是原选中会话的紧凑菜单，尚未提供“右击任意未选中行而保持当前选择”的完整上下文菜单。公开 MenuContent 缺少该原 owner 的 secondary-pointer/键盘上下文触发及 context anchor 契约，已提 [TekesUI #44](https://github.com/TekesApps/TekesUI/issues/44)；不在业务层伪造隐藏 trigger、复制 popup 放置或通用焦点/IME处理。Toolbar 的原 item事件/语义仍跟踪 #36/#5。

Source Views 的完整 row-context交互、SidePanel tabs、富内容、持久阅读恢复、最终等条件视觉及整体验收继续开放。PR #10 保持实验草稿，未据本阶段合并或打稳定tag。本阶段复用现有 target、debug0、incremental关闭，无新target，删除缓存/释放空间为0字节；保留固定输入、成功/失败日志、截图及验收程序。
