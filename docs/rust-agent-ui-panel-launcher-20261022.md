# SidePanel 空态入口与原生阅读复验（2026-10-22，实验状态）

公开 TekesUI main 与 `remediation-toolbar-checkpoint-20261021` 的 peeled revision 均为
`09b4bab44b6dfa65487250ea816b610b9df59095`，与当前精确 pin 相同，无需更换版本。
实际 Cargo 闭包核验8包/241文件，清单 SHA256
`44b3174022584d3c099a45b86892d17dbf948905476c0f038be104df12aefe46`。
#22/#28 仍 OPEN，但公开 reading/reveal 已可消费；本轮复验原生自动 reveal、分页及焦点。
没有依赖活动 TekesUI 工作树，也没有修改 SDK、ClientKit、Tekes 或 Kernel。

## Source 对照与组件边界

只读对照 ClientKit `Sources/ToolPanels/SidePanelLauncherView.swift`、
`ClientSidePanelHostView.swift`，以及 Tekes `SidePanelHostView.swift`、
`AppShell/MainWindowToolbar.swift`、`SidePanelLauncherView.swift`、
`ClientToolPanelHostAdapter.swift`。空态入口为 Summary、Review、Terminal、Browser、Files、
SideChat，按 Source 原顺序3列2行，隐藏目的项保留原槽。尺寸上限4倍 SDK button 行高、
3倍行高，section 留白、relaxed 间距、网格居中；AgentKit 提供这些业务布局约束，
公开 SDK `Grid::layout` 放置 cell，原 `ButtonContent` 负责文字、主题、绘制、焦点、输入、AX。
没有自画图标、复制字体色值、另写按钮或通用布局引擎。CoreText 组件验证覆盖4个主题、
1/1.4/2字号比例、220/400/720宽度；不等同于成对视觉验收。

`SidePanelLauncher` 的显式输入包括 owner、单调 revision、目的项标题、available/enabled。
独立宿主可用 `Workspace { id }`，不必虚构 RunScope；Run pane 则使用真实 Run owner。
可用性和产品权限由宿主给定，标签不授予能力。组件只发导航意图，不保存执行/导航/权限状态机。
原回调绑定组件实例、owner、revision 和活动控件 ID；关闭、隐藏、重建或 A→B→A 后旧回调失效。

集成调用顺序：先 `SidePanel::update`，再 `configure_launcher`，然后渲染；使用固定且与整个树
互斥的 namespace，配置后不能更换。仅 projection ready 且 selected=None 时挂载。
隐藏时只更新元数据并退役按钮，不缓存可交互隐藏控件。先 drain SDK actions，再更新受控输入；
异步派发前再检查 `accepts_launcher_intent`，不得把晚到回调解释成新导航。
`SidePanelIntent` 新增 `Launch` 分支，穷尽匹配的消费者需处理该分支。

Demo Summary 回调复用原打开路径，SideChat 复用原 presentation 请求/真实 scratch pointer；
Review/Terminal/Browser/Files 因没有产品 host adapter 而隐藏，不创建假能力。
组字期间导航禁用；合法 unmark 保留主草稿，不提交。原 `OPEN_SUMMARY` 调试入口仍保留，
不是另一套标准组件。unsupported SideChat 不再画成错误横幅；旧
`SidePanelLabels.unsupported_side_chat` 字段保留源码兼容，但不再绘制。pending/unknown/实际错误照常显示。

## 当前程序与证据

最终 SDK Grid 程序：`.runtime-fixtures/AgentKitPanelLauncherGrid20261022.app`，SHA256
`06404b6150d6b25eece03e47217ebb2db2c0b67ffdca5f13881826ab40140fb3`。
`grid-program.json` 的10个 Rust 源文件哈希与当前源码逐个匹配。

- UI workspace 515通过、1项既有 #49 Popover 接收门禁 ignored；fmt、strict all-target Clippy、
  demo build、边界和实际公开依赖闭包通过。独立 public API consumer 验证 owner/revision/实例
  隔离、失效动作、隐藏槽位、禁用按钮以及 SidePanel 挂载生命周期。
- 原生 Launcher 完整收据：原按钮 Summary 打开/关闭、单次 SideChat 创建及关闭确认、
  新控件身份恢复、旧动作拒绝；主草稿/编辑器 owner/opaque marked baseline 保留，无提交。
- 较大 tabs/Task/subagent 原生探针通过 phase14，包括 inactive Task 导航拒绝与 B 焦点保留。
  这只补足 owned fixture 的原生断言；此前 `5bf74a...` 程序的 phase13 失焦失败原样保留，
  不把本次成功归给旧程序，也不证明真实三端 Task pane 场景。
- 主阅读原生收据：20→44条，原分页按钮、原 turn/Start/latest reveal、前插锚点、向上阅读、
  A/B阅读与草稿、重连恢复、主编辑器焦点和 synthetic preedit baseline 均保留，无提交。
- 只读子历史：38→75条、pageSize50、尚未耗尽，前插及刷新锚点保留，组字时阅读仍阻断；
  跨 pane 复制不夺取主/子编辑器组字 owner。不是全量历史读取或全部耗尽验收。
- 同一最终程序正常 LaunchServices app 的系统拼音一次完整通过：`nihao` marked text →
  `你好` 确认 → 单次提交；逐键核对 bundle/PID/程序/前台与输入源，未强制激活或切换输入源。
  这是系统软件 IME，不能替代实体键盘、候选窗、VoiceOver 或长期稳定性验收。
- 同程序 Native0.9.3/Pi1.3.0/DSH0.2.1-rc.2 的公开协议数据→离屏 GPU smoke通过。
  Native 为明确 UI-only，完整 lifecycle/archive 不在范围；这些 smoke 不证明原生窗口阅读/IME。

初始手工 cell 放置程序 `e0edb0c...` 及其通过记录单独保留，随后改用 SDK Grid 后完整重验。
最终记录均以 `grid-` 为前缀，不混用两份程序证据。所有失败诊断、前台轨迹、日志、
收据和验收二进制在 `.runtime-fixtures/panel-launcher-20261022/` 及对应 app；
[机器记录](rust-agent-ui-panel-launcher-20261022.json)保留摘要。

## 继续开放的边界

Source imageAbove 图标尚缺原 Content 可消费的被动 Icon/renderer seam，跟踪已有
[TekesUI #47](https://github.com/TekesApps/TekesUI/issues/47)，不以 Unicode 或自绘替代。
Demo 内联 tabs/控制行仍是诊断宿主，不等于 Source 的原生窗口 toolbar；相关 #36/#5 仍开放。
[ProcessContainer #48](https://github.com/TekesApps/TekesUI/issues/48) 尚缺完整远端交互，
本轮仅同进程原 Content；#49 Popover 门禁仍未验收。完整 Views、产品 adapter、视觉及无障碍
验收未完成，PR #10 保持 draft，不合并、不打 tag。

复用现有 target，未创建新 target，未删除缓存，实际释放空间0。保留固定依赖、源码、
WIP、全部验收程序及成功/失败证据。
