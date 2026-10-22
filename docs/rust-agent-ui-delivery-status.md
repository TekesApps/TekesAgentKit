# Rust Agent UI：当前接入范围与交付状态

本页以已提交源码 `cb1c28e`、[卡片/桥接增量](rust-agent-ui-tool-card-20261022.md)、[限高实验](rust-agent-ui-bounded-details-20261022.md)、[展开分区事实](rust-agent-ui-tool-details-20261022.md)及[独立分区控件](rust-agent-ui-tool-section-controls-20261023.md)的逐文件记录为基线，按用户完整目标保留验收范围。当前可供实验宿主接入，**完整交付尚未达到**。
组件存在、headless/离屏通过或单个 Issue 的 OPEN/CLOSED 都不能单独证明完整 Views 对齐。
历史记录按原 pin、程序和输入范围保留；本页是入口，详细阶段账本仍见[接入契约](rust-agent-ui-contract.md)。

## 依赖与运行身份

| 输入 | 当前固定值及限制 |
| --- | --- |
| 已发布 AgentKit | `v0.3.8`，根 workspace 版本同为 0.3.8；本分支包含尚未发布的共享增量，不能把 UI 当作未修改的 v0.3.8 消费者。 |
| UI packages | `ui/` 为独立 workspace，0.0.0 / publish=false；开发使用显式仓库内 path，固定整个 AgentKit revision。正式宿主须等待包含所需 API 的新不可变发布 pin。 |
| TekesUI | 公开 `50ec49f3446924fdbc66b3cff22cd8cfe150fe7c`，Cargo.lock 中 8 包；`ui/tekesui-source.json` 核验 243 输入，清单 SHA `3ad8aa98a567679b775e78b634f45888c9eb70ffd05708a4f781c0ce7264471c`。 |
| 原生运行时 | Pi 1.3.0、DSH 0.2.1-rc.2；Native 当前程序实测 Kernel 0.9.3 为明确 UI-only，完整协议/子历史基线与 UI 验证分开，不能据此宣称 0.9.3 全生命周期通过。 |
| 宿主路径 | 当前 Sidebar / RunPort / SidePanel 使用同进程原 ContentHost；未按 ProcessContainer 同/跨进程契约验收。RunSurface/ComponentDemo 是测试宿主。 |

## 按完整目标核对

| 要求 | 当前实现/证据 | 距离完整交付还差什么 |
| --- | --- | --- |
| Composer、发送/取消、流式回传 | 原 TekesUI editor + RunBinding/RunController；归档/中断、草稿、请求身份、模型/附件等有组件与宿主用例。 | 本轮源码的完整原生/IME组合；上下文高级入口按源规则逐项验收，缺少宿主能力时通过明确输入/回调接入。 |
| Transcript：历史/推理/工具/分页/阅读 | 混合 Content、原阅读事务、canonical 身份与只读子历史；公开 page/reveal 已消费，工具关闭边界、共享 row 与五类卡片事实有回归。 | 独立 Input/Output/view 原字段与复制通道已接入；富内容、#54 分隔线和最终 code-header 样式未完成，`rich_content=false`；选中富内容 #25、嵌套阅读意图 #53、新建 Host 的持久 checkpoint #42、当前原生与配对布局。 |
| Input / output / footnote 排版 | 用户气泡/aside 右、机器内容/aside 左；共用列、单行、备注限宽与保留控件有独立 consumer。 | 真实 Pi 目标标签在窄列仍不可见，严格命令退出 1；#47 原 glyph/紧凑动作消费，完整时间/目标/状态可见性。 |
| Indicators 与回合导航 | 原连接/运行/等待/失败/用量事实、TurnRail/Jump 与原 SDK reveal，非像素估算。 | 完整 hover/selection/chrome、可达性和当前程序原生对照；#46/#5 等。不能把已有单行状态当作完整 rail 的证明。 |
| RunPort 组合与完整 Views 布局 | 正式 RunPort 组合 Composer/Transcript/Indicators，整帧 preflight/阅读发布；第二 consumer 无需 RunSurface。 | 完整 ClientKit 对应层次/selection/chrome 的逐项同内容对照；当前收据仍明确 `completeRunPort=false`。 |
| Sidebar 会话/活动/置顶/共享操作 | 受控目录/恢复/搜索/分组、稳定 menu value、原共享 action/CAS、分页/选中补齐和 reveal。 | 非当前行的原上下文菜单 #44、完整当前原生/AX验收；不把 compact toolbar 的缺口套到已经消费的普通 MenuContent 上。 |
| SidePanel Task/Subagent/SideChat/Summary tabs | 同一 SidePanelInput + SidePanelNavigation；动态 pointer、retained editor、关闭确认和迟到回包门禁有宿主/第二 consumer 用例。 | SideChat 家族原生菜单未通过（最近一次 phase 20 触发启动计时期限；见[续验](rust-agent-ui-native-handled-20261022.md)）；完整窗口 toolbar/Panel chrome 及 #48。 |
| Task/child 的可写与有界只读来源 | TaskPaneBinding、ChildPaneBinding、SideChatBinding 复用现有 presentation；有界历史无 Composer、不用 parent 正文代替。 | 每种 loading/missing/error/ready 的当前完整原生组合；Native 真实多消息有界路径与 UI-only 验收分别记录。 |
| Summary sections / rows / actions | 受控 SummaryInput/Context、展开/用量/metadata/宿主 slots、原 intent 与 source ownership。 | #47 状态附件/图标、#25 语义内容、#30/#38/#46 部分焦点/指示/hover组合与最终产品对照。 |
| 反馈初读/重连/CAS | 自动按来源/epoch读取，普通 aside 无常驻 Refresh；加载失败保留旧记录且不可写，明确刷新恢复。 | 本轮已提交源码的真实故障/原生验证；[最近三端成功证据](rust-agent-ui-feedback-recovery-20261022.md)属于 4097e4f，不转移程序身份。 |
| ProcessContainer 同/跨进程 | 当前没有正式适配；8 包闭包不含 module-runtime/app-protocol。 | #48 固定公开 SDK 中 `new_view` 仍声明无远端 keyboard/IME/AX，V1 的 keyboard/scroll/text_ime/accessibility 为 false；需 SDK 交付后再接入三个业务容器并独立验收。 |
| 原生、中文组字、阅读/重连、第二 consumer | 有多个冻结程序的分项成功、失败和完整收据；组件/第二 consumer 在 CI 跑，未打开桌面。 | 同一交付程序的完整 Native/Pi/DSH + 当前系统 IME、最终配对布局；跨进程/VoiceOver 未证明。用户已处理系统提示；本次活动窗口期限失败，未继续系统组字。 |
| review、CI、合并/tag | PR #10 draft；cb1c28e 三项 Linux CI 成功、三项 macOS 检查在严格阅读步骤失败；不覆盖本轮增量。root 241 为未改根代码的前阶段证据，本轮 UI 559通过/1忽略；严格嵌套阅读仍失败；新提交 CI 另按当前 Checks 核对。真实三端离屏反馈证据仍属于冻结的 4097e4f。 | 完整 review、全部要求与验收审计、新发布 pin；仍有明确失败/缺失，不能合并或打完成 tag。 |

## 后续工作顺序

1. **AgentKit 可先做**：按上表 source View 逐项补状态/回调与布局验证；修正过期接入说明，保持原请求/指针/权限和 presentation，不让宿主另写业务状态机。
2. **上游交付后消费**：优先 #48 完整交互式 ProcessContainer、#47 原 glyph/紧凑动作，再按场景接收 #25/#42/#44/#45/#49 等。读取公开能力和实际代码，不能仅凭 Issue 状态决定能否升级。#22/#26/#28 的已有阅读/preflight/reveal能力已经消费，保持回归。
3. **桌面可用后**：用最新冻结程序补原生菜单、发送/取消/流式、分页/追加、A/B草稿、重连和系统中文组字；有焦点/输入异常就保留失败，不自动重激活或重放提交。
4. **固定程序交付**：ClientKit 同内容/尺寸/主题/scale 对照、第二 consumer、同进程及跨进程分别留证；通过完整要求审计后再 review、合并、发布不可变 tag。优先复用 target，保留程序/日志/截图和 WIP。

新宿主的状态输入、回调与焦点责任见[短接入说明](rust-agent-ui-host-integration.md)。
[审计快照](rust-agent-ui-delivery-status.json)记录源码、依赖、公开 Issue 和证据来源；它是未完成项清单，不是全量验收凭证。
