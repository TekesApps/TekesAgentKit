# SidePanel 导航与宿主 header 接入（实验）

新增业务组件 `SidePanelNavigation`。它消费与正文相同的 `SidePanelInput`，输出已有
`SidePanelIntent`；tab 文档/顺序由宿主管理，任务、子代理、SideChat 的执行和指针仍由
既有 presentation/bindings 管理，没有另一套导航或 Run 状态机。

对照当前 ClientKit `ClientSidePanelHostView` / `SidePanelTab` 和 Tekes 的
`MainWindowToolbar` 实际注册行为：普通 Summary/Task/Subagent tab 按源顺序显示；
SideChat 使用一个家族菜单，首项 New SideChat，然后 separator 和具体成员；成员文字
统一为 SideChat，checked 表达当前成员，家族入口表达当前选择。关闭入口针对当前 tab。
不把文件、浏览器或窗口 fill/collapse 等宿主产品能力新增为 Agent 枚举。

## 组件所有权与接入

```rust
let mut panel = SidePanel::new(panel_namespace, labels, 4);
panel.set_header_placement(SidePanelHeaderPlacement::Host);
panel.update(&publication)?;
let mut navigation = SidePanelNavigation::new(toolbar_namespace, Default::default());
navigation.update(&publication, host_interaction_enabled)?;
// 用互不重叠的 namespace 挂载原 Content：
// Content::new(panel.id(), panel)
// Content::new(navigation.id(), navigation)
```

宿主分配 toolbar 区域与面板正文，并从**同一 publication** 更新二者；两次更新与原
ContentHost render 成功后才发布该帧。`content_top_inset` 仍由窗口宿主提供。
Host 模式移除诊断 tab/关闭/新建控件的挂载子树，不留两行空白；正文、原 editor 与
content token 保留。关闭、换 parent、指针变更和 SideChat 未确认丢弃规则仍复用原逻辑。
默认 `InlineDiagnostic` 只保留旧 demo 的可运行路径，不能把它称为窗口 toolbar。

在原事件边界排空富事件队列，不能先消费丢失菜单 payload 的 legacy queue：

```rust
let records = host.drain_action_records();
let navigation = host.get(navigation_id).unwrap()
    .component::<SidePanelNavigation>().unwrap();
let intents = navigation.take_intents(&records, &host);
// 在当前 pane / native preedit 屏障允许时，交给既有导航/SideChat handler。
```

必须把组件按其 `id()` 挂在传入的原 Host 中。消费时同时核对原组件实例、SDK mount、
菜单 model revision 和每次事件的稳定 value；不轮询 `MenuContent.selected`。
普通 tab 原实例在仅选择/字号/布局变化时保留；目标、能力或菜单模型改变则退役旧 ID。
关闭目标改变、同号重挂载、保留在 Host 外的旧组件和旧菜单动作不能重绑新会话。
`enabled` 是宿主交互输入；未就绪投影不能由新导航发起打开/创建/关闭动作。
SideChat 创建、Closing/unknown 等许可直接读取既有共享快照，不新增本地 pending 状态。
`SideChat` / `SideChatPhase` / `SideChatSnapshot` 重新导出原共享类型，未复制 DTO。

## demo 与协议验证

demo 增加 `--components --panel-navigation-toolbar`，将原 Kit 控件放到宿主分配的
in-content toolbar 区域，正文切为 Host 模式；仍是测试宿主，不是完整产品窗口。
`--offscreen` 可验证 GPU；实时脚本的对应选项是 `--ui-panel-navigation-toolbar`，
会检查导航/正文输入一致、旧诊断控件隐藏和原 body slot 挂载。该脚本选项明确拒绝与
仍使用旧诊断 header 的 native probes 混用，避免转移验收范围。

最终程序 SHA256 为 `0583ddab0447604f20148dc1141eb17cc581c5a80ae6fffee082d463aac630dd`。
Native 0.9.3 UI-only / Pi 1.3.0 / DSH 0.2.1-rc.2 的真实公开协议数据到此程序的原
控件/离屏 GPU 路径通过，使用隔离本地 provider。它们验证 owned 主会话/Summary 路径；
不代表任务、子代理、SideChat 全部真实三端动态 tab 或原生窗口菜单已验收。

## 验证与尚未交付的边界

- UI workspace 全量 531 passed、1 个既有 #49 ignored。随后补强原 Host/组件实例校验，
  最终 28 项定向组件/宿主/公开 consumer 测试、严格 all-target Clippy 和 demo build 通过。
- 第二公开 consumer 验证原菜单连续 A/B 的事件时间值、选中/关闭对象与权限变化、
  同号 remount、旧组件和未确认 SideChat 丢弃拒绝；这是 SDK semantic action 验证。
- CoreText consumer 覆盖 4 主题 × 3 字号、770→320→180→770 宽度；保留原 pane、
  toolbar group、按钮、另一 Composer 焦点与完整 marked baseline，source/AX 名称完整。
  窄宽使用原 SDK 裁剪/省略，不宣称完整 overflow 或所有按钮在窄列均可见。
- 实际固定公开 TekesUI `50ec49f3446924fdbc66b3cff22cd8cfe150fe7c`，8 包/243 输入
  与清单摘要核对通过。AgentKit 使用本仓库开发路径，共享增量尚未作为新 tag 发布。

窗口级注册接入已补充到 [TekesUI #14](https://github.com/TekesApps/TekesUI/issues/14#issuecomment-6338249373)：
已明确核对统一标题栏、原生 chrome 与 row/inset API，不要求另写 NSToolbarItem 实现；
请上游明确完整支持路径及窗口生命周期。原紧凑 ToolbarControl item 事件/语义仍跟踪 #36/#5，
被动图标 #47、ProcessContainer 远端交互 #48 未交付。这里只消费公开 Content toolbar/menu，
不把普通内容绘制当作窗口注册，也不复制 overflow、焦点、IME、AX 或进程引擎。

本轮没有原生窗口、实体键盘、候选窗或 VoiceOver 通过声明。此前原生失焦原因及三端
流式阅读未完成项保持原结论，未合并/打 tag。完整 UI 目标仍未完成。

[机器收据](rust-agent-ui-side-panel-navigation-20261022.json)记录程序、源码/参照、日志与截图摘要。
本地证据位于 `.runtime-fixtures/side-panel-navigation-20261022/`；363 项仓库源码/验证输入
有独立留证清单，不把该清单称为 rustc 的实际依赖闭包。早期 `a535b154…` 程序与日志保留，
最终三端结果取自独立 `three-final.log`。复用共享 target；两个已实际验收的冻结程序、截图和
收据约占 140 MiB，均保留。未创建独立 target，未删除证据或现用缓存，释放空间为 0。
