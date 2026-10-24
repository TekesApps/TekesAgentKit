# SidePanel 宿主导航收口（2026-10-22，实验状态）

对照只读 ClientKit `ClientSidePanelState.swift` / `SidePanelTab.swift` 与 Tekes
`ClientAgentHostAdapters.swift`：Summary 同样可关闭；既有 tab 再次打开只选择而不重排；
关闭当前项后选最后一个剩余项，全部关闭后保留空文档；按 thread 保留导航文档。
本轮不修改这些参考仓库，也不引入 Swift 构建。

demo 此前忽略 Summary 关闭，关闭任务/子代理一律回 Summary，并按 BTreeMap 的 ID 重排目录。
现改为宿主有序 tab 文档，保留现有 Run/TaskPane/ChildPane/SideChat bindings 作为唯一执行、
pointer 与草稿来源。全部关闭后普通 sync 和 A→B→A 不重新生成 Summary；
原 SDK 按钮 `OPEN_SUMMARY` 是 demo 的显式宿主导航入口，不是新的共享 launcher。
重新打开既有 Summary 不改变其位置；已关闭 Summary 在末尾重开。

SidePanel 的公共受控输入不变：宿主提供单调 revision、原 parent、projection_ready、
有序 tabs、可为空的 selected 和原 SideChat 快照。先排空并捕获原控件 intents，
再更新输入。关闭后的旧 `PanelContentToken` 不能安装晚到内容；显式重开获取新 token。
新增独立 consumer 验证这一完整过程，不依赖 demo 内部导航状态。

任务关闭只撤销原加载 authority，子代理关闭只 retire 原 pane；不取消底层 Agent 执行。
SideChat 仍由原 controller 确认关闭，不提前丢弃 pending/unknown pointer 或草稿。
晚到的有效 pointer 可以进入原目录，但不能重选用户已经关闭的文档。
关闭确认导致 fallback 时仅在旧 pane 拥有焦点的情况下清除它，主 Composer 的原
编辑器、focus 和中文 preedit baseline 不受异步确认影响。该组合有 CoreText 回归。

## 本轮验证

- UI workspace 503项通过，1项 #49 原 Popover 接收门禁显式忽略；严格 all-target Clippy、fmt、
  build、边界与实际8包/241文件公开源清单通过。新增6项宿主回归及1项独立 consumer。
- 同一冻结程序 `.runtime-fixtures/AgentKitRevealCurrent20261022.app`，SHA256
  `b1092549757381b3df33d65ddf377590acdc0402225e912f6135cf5f0deb647a`：
  原生窗口通过原控件关闭/重开、打开顺序、最后剩余项、空文档、原 Task 编辑器与主草稿保留，
  无提交。Task 数据通过原 TaskPaneBinding 和宿主异步命令取得，不另建任务状态机。
- 同一程序原生 owned SEP 阅读20→44、前插锚点、向上阅读、Start/Latest、A/B 阅读与草稿隔离、
  重连、原焦点/编辑器/合成中文 preedit baseline 通过；Sidebar 原目标自动 reveal、选中项补读、
  失败保留与 marked baseline 通过。没有用改焦点或像素补偿实现滚动。
- 同一程序通过真实 Pi1.3.0、DSH0.2.1-rc.2、Native Kernel0.9.3 的公开数据到离屏 GPU 路径。
  Native 为明确 UI-only 范围；不是三端原生窗口分页/子 pane/执行生命周期验收。
- 系统拼音产生 `ni hao` preedit，随后窗口失焦，完整断言失败（exit3）。不报告软件 IME 全流程通过。
  上一轮全部失败记录保留在 `.runtime-fixtures/reveal-current-20261022/`，本轮成功与失败记录分别归属；
  本轮原生成功不覆盖上一轮失败，也不证明后台程序抢焦点的根因。

当前公开 SDK 精确 pin 仍是 `09b4bab44b6dfa65487250ea816b610b9df59095`，#22/#28 API 已消费。
ProcessContainer #48 与交互 Popover #49 仍 OPEN；未修改 SDK、复制基础实现或扩展实际依赖闭包。
跨进程 UI、实体键盘/候选窗、VoiceOver、全部 Source tabs/chrome 和完整视觉对齐仍未完成。
本轮没有证明所有异步打开请求的全局排序、生产文档缓存预算或销毁后阅读 checkpoint（#42）。

原始日志、前台轨迹、源码/程序哈希及收据位于 `.runtime-fixtures/panel-navigation-20261022/`；
摘要见[机器记录](rust-agent-ui-panel-navigation-20261022.json)。复用现有共享 target，未新建 target，
未删除缓存，释放空间0。PR #10 保持 draft；本阶段不合并、不打 tag，不视为整个目标完成。
