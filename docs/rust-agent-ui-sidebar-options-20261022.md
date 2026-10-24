# Sidebar 原选项菜单接入

公开 TekesUI 精确 pin 仍为 `09b4bab44b6dfa65487250ea816b610b9df59095`。重新核对其源码后确认，原 `MenuContent` 已具备 #36 的子菜单、checked/disabled、稳定 value path 和事件时间 model revision；不能继续把历史“等待公开菜单 API”作为 Sidebar Sort/Group 的阻塞。

对照当前 ClientKit `Sources/Composer/Agent/ClientSessionSidebar.swift` 的 `ClientSessionSidebarControls.makeMenu/selectOption`，以及 Tekes `ClientAgentHostAdapters.swift` 和 `AppShell/MainWindowToolbar.swift`：新增可独立挂载的公开 `SidebarOptions`，按 Sort by → Recent activity / Title A–Z / Title Z–A、Group by → None / Primary Folder 声明原菜单。当前选择的勾选来自原偏好快照；缺本地化标题比较器时禁用标题排序，偏好只读时禁用触发器。颜色、字体、按钮、菜单输入、浮层、焦点与语义由 SDK 提供，不自行实现菜单或窗口定位。

## 接入契约

宿主为一个固定偏好存储分配独立 namespace，并把选项控件放在共享 Sidebar 的外层布局中；切换偏好存储 authority 应重建对应控件，不把不同存储的 revision 混用。`SidebarOptionsLabels` 提供文案，`update(snapshot, writable, title_sort_supported)` 接受同一 `SidebarBinding` 的原快照。相同输入是 no-op，不取消已打开的菜单；不相关偏好、权限或能力变更也会退休原 menu owner，防止旧选择采用新 CAS baseline。

事件边界必须使用原 `ContentHost::drain_action_records()` 一次，并从这些记录导出其它控件所需的旧 `ContentAction`；先调用 `drain_actions()` 会丢失 payload。`SidebarOptions::take_intents` 校验原目标及原 SDK model revision，只接受原记录的稳定 path，不回读 selected。每个 `SidebarOptionsIntent` 包含 `SidebarPreferenceChange` 和 `reset_paging`：保留原 expected 快照与其它所有偏好，仅改变该选择对应字段。写回仍用原 `SidebarBinding::set_preferences`；冲突不自动重基线或重试。

ClientKit 对重复选择同一个排序也重置普通分页，因此排序意图始终携带 `reset_paging=true`；demo 调用原 `Sidebar::reset_paging`，随后发送原偏好命令。分组不会借机重置排序或 pin 顺序。标题比较继续由宿主注入；`Sidebar::title_sort_supported` 只暴露该能力事实。

demo 已挂载此公开组件并使用原偏好观察/回传。第二公开 consumer 提供相同事件记录消费示例，未依赖 demo 或 SDK 私有模块。宿主的 IME 屏障阻止组字期间打开菜单，不以替换编辑器解决焦点。

## 验证及保留边界

UI workspace 436 项和 strict all-target Clippy 通过，含四项新增专项：连续选择后延迟 drain 仍保留两条原 path/快照；无关偏好或权限变化后旧记录拒绝；另一同 namespace 菜单的记录拒绝；checked、缺比较器禁用、相同输入保持；原持久 binding 写盘、重复变更 CAS 拒绝及中文草稿/零提交。第二 consumer 的公开事件适配也由专项调用。

原生 fixture 在实际收到组字中的菜单位置 pointer-up 后才检查屏障：原 focus、opaque baseline 和未打开状态保持。Unmark 后，通过原 AppKit 鼠标路径打开原两层菜单，选择标题升序和文件夹分组，原存储与组件发布确认、原会话行实例和草稿保持、无提交，窗口 exit 0。带截图的同程序运行也通过，截图已检查。它是 fixture 诊断宿主，不是配对 ClientKit 视觉验收，也不是实体输入法、候选窗或 VoiceOver 验收。

同一程序另一次完整 local 阅读探针在 phase14 失去激活而未完成；旧窗口切换问题继续保留，未弱化断言或称阅读全流程通过。当前变化没有完成 SidePanel tabs、Move Pin Up/Down 原菜单入口、富内容、持久 reading checkpoint 或整个 Views 对齐。

原 `ToolbarControl` 仍只有可变 take_action 和根 Group，不能直接等同于已有 `MenuContent` 的稳定记录/逐项语义。已向 [#36 追加准确缺口及验收](https://github.com/TekesApps/TekesUI/issues/36#issuecomment-6320403291)，语义部分关联 #5；没有修改 SDK，也没有重复开 issue。

证据、冻结程序、截图、来源哈希及成功/失败日志保留在 `.runtime-fixtures/sidebar-options-20261021/`，程序为 `.runtime-fixtures/AgentKitSidebarOptionsCheck.app`。本阶段复用现有共享 target，无新 target，未删除用途不明确的缓存，删除/释放为0字节；不重复计入上一阶段已释放的54.23 MiB。PR #10 保持实验草稿，完整 review/合并/tag 尚未完成。
