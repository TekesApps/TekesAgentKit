# TekesUI 3bef 公开精确依赖接收（2026-10-22，实验状态）

本轮精确升级至公开 `3bef66754a256e7723887de7d33518f5122805ad`（toolbar/split checkpoint）。
6个直接和2个传递 SDK 包由 Cargo 获取同一完整 rev；没有活动工作树或 path override。
AgentKit 最新已发布契约仍为 v0.3.8，public main 为 `f1daa0ffcf6ced41e21f351e22845455ca78006c`，
UI仍为0.0.0/publish=false，不把此 revision pin 称为 AgentKit UI 稳定发布。

## 公开来源与差异审查

匿名 commit/tree/codeload 下载逐项匹配2,792个 Git blob，归档 SHA256
`4d941f25772e5607d7d8a320f6229eb2ead7ddd8235577f1e010645f2f5809d8`，共112,695,768字节。
Git root tree 为 `30ac7070ff5978e5365327a9c588c638a2b15395`；无截断、无链接/设备或未列文件。
消费侧保留最小固定源码8包/242文件，清单 SHA256
`e6f0dbcdfdadcc8e4a67a46cefd1a814956461d3896a78f8469091c4420a6824`。
实际 Cargo metadata 指向新 Git source，逐文件核验新 checkout 与该清单一致。

审查从09b4bab到本提交的唯一 commit、实际消费包 production diff 及上游独立接收文档：

- 新公开 `ToolbarContent` / 稳定 `ToolbarGroup`，原 Content 子控件保留，语义区域放置及单行标签；
  原 Toolbar 的语义 part/help/focus 与原 action 身份；不等于整个窗口注册或任意自定义内容溢出已完成。
- `ButtonTextRenderer::resolved_font_size` 转发、原 Button Minimum/Exact 几何跟随 type scale；
  原 SplitView 的 consumer minima、分隔符与分离的真实 hit/cursor 区域。
- 原生 toolbar 对齐/zoom 交接/close handler 生命周期/cursor tracking；新窗口 toolbar 对齐为 opt-in。
  本轮没有自写原生控件位置、大小、焦点、IME或split引擎，也没有启用新的窗口标题栏策略。
- design-export 的生成输入更新已进入清单。reading/reveal/text_input 内容未改变，module-runtime/
  app-protocol 不在本次8包闭包；不能据此宣布 ProcessContainer #48 已交付。

上游3bef push CI success 是上游证据；它不替代本消费侧原生验收。
原09清单完整保存在 `ui/baselines/tekesui-09b4bab44b6dfa65487250ea816b610b9df59095.json`。
Cargo初次update额外改选Windows依赖连接，已保存诊断并恢复：最终lock差异仅8个SDK source行，
registry版本/依赖连接保持原样，`--locked --offline` 重新解析通过。

## 当前消费验证

UI workspace完整518项通过、1项既有#49门禁ignored；strict all-target Clippy、fmt、demo build、
边界与实际闭包核验通过。未修改生产 Rust 组件、presentation、binding或原生demo代码。

新增独立 public consumer 将原 SidebarOptions 放入 SDK ToolbarGroup，Composer在同一原Content树中。
覆盖4主题和1/1.4/2字号：toolbar行高随原字体比例变化，分组重排及1100/770/320宽度往返时，
原 options/Composer 实例、焦点和完整 marked baseline 保留；合法commit后，原Menu分组/选项
仍产生原typed preference意图。它证明这条受支持组合路径，不冒充任意溢出、原生Window toolbar、
VoiceOver或SidePanel最终窗级tab排列。

显式运行原#49 opt-in门禁仍退出101：预期弹层原按钮51002，实际收到背景51003。
普通suite继续将它标为已知ignored，不宣称基础事件路由已经通过，也不复制popover实现。

## 当前程序的原生证据及未完成范围

冻结 `AgentKitTekesUI3bef20261022.app`，SHA256
`a0da587c27d60345fe24264dab6e2dcfa8e7abc905c9e2df271ad41778cf540f`。
相同程序/SDK分别得到以下完整收据；所有 provider 和 sessions 均为隔离本地受控来源，无付费调用。

| 模式 | Pi1.3.0 | DSH0.2.1-rc.2 | Native0.9.3 UI-only |
|---|---|---|---|
| public分页、A/B草稿与阅读、原编辑器、重连 | 通过50→56 | 通过50→59 | 通过50→56 |
| 原生反馈/中文备注/原Composer/组字屏障 | 通过 | 通过 | phase9失焦失败，未完成 |
| 流式向上阅读/原锚点/合成组字/重连不重放 | 通过，3次追加 | phase6失焦未完成 | 未执行 |
| 流式发送/原文字owner/选区/settlement/重连 | 通过 | 未执行 | 未执行 |
| 发送/取消/aborted/错误/显式恢复 | phase2失去激活超时 | 未执行 | 未执行 |

三次组合尝试均退出1，没有完整批次通过。Pi取消窗口的前台切到Codex；DSH流式阅读切到
Codex/UserNotificationCenter；Native备注切换后切到 `com.tekesui.editable-number.validation`，
随后ExpectText返回None。只陈述当前轨迹，不将所有历史失焦归因为SDK或某一外部程序。
原assertions/45秒deadline未放宽，没有强制抢焦点或继续向非原编辑器输入；所有失败原样保留。
检测到其他原生验收同时使用前台后，停止新增窗口，未中断或操作其他任务。

当前pin的owned原生fixture/正常app系统拼音尚未重跑；此前09程序的成功不转移到3bef。
本轮合成preedit收据不等于实体键盘/候选窗、VoiceOver、长期稳定性或成对视觉。
Native明确不执行完整lifecycle/archive/child协议证明。当前原生全套仍未完成，后续必须补验。

## 后续边界及保留

ToolbarContent/group现可供host组合，完整窗级tabs/chrome仍需实际接入和验收；
Menu #36和AX #5的全部要求不由此自动关闭。ProcessContainer #48、Popover #49、Icon #47、
窄列脚注及长备注限制、剩余Views显示规则与最终审计仍开放。PR #10保持draft，本轮不合并/打tag。

源码归档、最小快照、原/新manifest、source diff、metadata、程序、成功/失败/前台轨迹、截图和日志
保留在 `.runtime-fixtures/tekesui-3bef-intake-20261022/`；摘要见[机器记录](tekesui-3bef-public-intake-20261022.json)。
继续复用既有UI/root targets，未新建target；共享UI target约27GiB，固定来源归档/证据目录约116MiB。
只读检查可确认link-complete rcgu对象候选为0；其余当前缓存和验收可执行/快照保留，本轮释放空间0。
