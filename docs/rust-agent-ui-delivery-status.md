# Rust Agent UI：当前接入范围与交付状态

本页以已提交基线 `f61cfcb` 与本轮共享 Activity 展示判定增量为准，按用户完整目标保留验收范围。
当前可供实验宿主接入，**完整交付尚未达到**。组件存在、离屏通过或单个 Issue 关闭都不能单独证明完整 Views 对齐。
最新源码增量见 [Activity 展示规则](rust-agent-ui-activity-policy-20261023.md)：正常/异常终止默认展开、回合内追加输入和 direct task-source 使用共享判定；物理分区与 settled disclosure 尚未完成。
原生、真实协议、离屏 GPU 和跨进程证据分别记录，不转移冻结程序身份。本轮结果见[当前程序续验](rust-agent-ui-current-validation-20261023.md)。

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
| Transcript：历史/推理/工具/分页/阅读 | 混合 Content、原阅读事务、canonical 身份与只读子历史；公开 page/reveal 已消费，工具关闭边界、共享 row、报告/完整计数选择规则与五类卡片事实有回归。 | 独立 Input/Output/view 原字段与复制通道已接入；真实报告来源、连续分组的完整输入/原生验收、富内容、#54 分隔线和最终 code-header 样式未完成，`rich_content=false`；选中富内容 #25、嵌套阅读意图 #53、新建 Host 的持久 checkpoint #42、当前原生与配对布局。 |
| 实时思考模式 | 共享 None/Compact、来源绑定 Frame、独立 OutputMessage 与原归一化来源工厂已实现；Root Run/Transcript/Pane API 和第二 consumer 的 fixture 路径通过。Journal anchor 与 Pi ancestry 按实际 projector 定义，不使用 UI/buffer 下标作身份。 | TranscriptContent/RunPort/只读 pane 整帧输入及 prepared-plan 门禁已接线，demo 显式消费 Compact；reasoning carrier 与单详情/空白行已对齐；消息容器/独立工具的平面展示顺序已接入原 planner；独立 reasoning/narration kind、narration 聚合、plane 拆分与真实三端整帧验收仍未完成；裁剪/未知来源不能猜补。 |
| Input / output / footnote 排版 | 用户气泡/aside 右、机器内容/aside 左；共用列、单行、备注限宽与保留控件有独立 consumer。 | 真实 Pi 目标标签在窄列仍不可见，严格命令退出 1；#47 原 glyph/紧凑动作消费，完整时间/目标/状态可见性。 |
| Indicators 与回合导航 | 原连接/运行/等待/失败/用量事实、TurnRail/Jump 与原 SDK reveal，非像素估算。 | 完整 hover/selection/chrome、可达性和当前程序原生对照；#46/#5 等。不能把已有单行状态当作完整 rail 的证明。 |
| RunPort 组合与完整 Views 布局 | 正式 RunPort 组合 Composer/Transcript/Indicators，整帧 preflight/阅读发布；第二 consumer 无需 RunSurface。 | 完整 ClientKit 对应层次/selection/chrome 的逐项同内容对照；当前收据仍明确 `completeRunPort=false`。 |
| Sidebar 会话/活动/置顶/共享操作 | 受控目录/恢复/搜索/分组、稳定 menu value、原共享 action/CAS、分页/选中补齐和 reveal。 | 非当前行的原上下文菜单 #44、完整当前原生/AX验收；不把 compact toolbar 的缺口套到已经消费的普通 MenuContent 上。 |
| SidePanel Task/Subagent/SideChat/Summary tabs | 同一 SidePanelInput + SidePanelNavigation；动态 pointer、retained editor、关闭确认和迟到回包门禁有宿主/第二 consumer 用例。 | SideChat 家族菜单的本地协议演示已通过（phase 25；见[原生续验](rust-agent-ui-native-resumed-20261023.md)）；真实三端及最终源码仍待验收；完整窗口 toolbar/Panel chrome 及 #48。 |
| Task/child 的可写与有界只读来源 | TaskPaneBinding、ChildPaneBinding、SideChatBinding 复用现有 presentation；有界历史无 Composer、不用 parent 正文代替。 | 每种 loading/missing/error/ready 的当前完整原生组合；Native 真实多消息有界路径与 UI-only 验收分别记录。 |
| Summary sections / rows / actions | 受控 SummaryInput/Context、展开/用量/metadata/宿主 slots、原 intent 与 source ownership。 | #47 状态附件/图标、#25 语义内容、#30/#38/#46 部分焦点/指示/hover组合与最终产品对照。 |
| 反馈初读/重连/CAS | 自动按来源/epoch读取，普通 aside 无常驻 Refresh；加载失败保留旧记录且不可写，明确刷新恢复。 | 本轮真实三端离屏组合验证见当前续验；真实故障恢复仍属 4097e4f 冻结程序，反馈的当前原生交互仍待验收。 |
| ProcessContainer 同/跨进程 | 当前没有正式适配；8 包闭包不含 module-runtime/app-protocol。 | #48 固定公开 SDK 中 `new_view` 仍声明无远端 keyboard/IME/AX，V1 的 keyboard/scroll/text_ime/accessibility 为 false；本轮复核公开 50ec49f 仍如此；需 SDK 交付后再接入三个业务容器并独立验收。 |
| 原生、中文组字、阅读/重连、第二 consumer | 有多个冻结程序的分项成功、失败和完整收据；组件/第二 consumer 在 CI 跑，未打开桌面。 | 同一交付程序的完整 Native/Pi/DSH + 当前系统 IME、最终配对布局；跨进程/VoiceOver 未证明。本轮 f44 冻结程序已通过本地原生分页/焦点/草稿/重连、SideChat 菜单和系统拼音；不等于真实三端或跨进程验收。 |
| review、CI、合并/tag | PR #10 draft；f5dfd8e 两项 Linux CI 成功、两项 macOS 严格 #53 阅读步骤失败。本轮根 276、完整 UI 582 通过/1 原有忽略，根/UI strict Clippy、格式/边界/SDK 输入检查通过；当前严格阅读仍退出 1。 | 完整 review、全部要求与验收审计、新发布 pin；严格失败仍在，不能合并或打完成 tag。 |

## 后续工作顺序

1. **AgentKit 可先做**：按上表 source View 逐项补状态/回调与布局验证；修正过期接入说明，保持原请求/指针/权限和 presentation，不让宿主另写业务状态机。
2. **上游交付后消费**：优先 #48 完整交互式 ProcessContainer、#47 原 glyph/紧凑动作，再按场景接收 #25/#42/#44/#45/#49 等。读取公开能力和实际代码，不能仅凭 Issue 状态决定能否升级。#22/#26/#28 的已有阅读/preflight/reveal能力已经消费，保持回归。
3. **桌面可用后**：用最新冻结程序补原生菜单、发送/取消/流式、分页/追加、A/B草稿、重连和系统中文组字；有焦点/输入异常就保留失败，不自动重激活或重放提交。
4. **固定程序交付**：ClientKit 同内容/尺寸/主题/scale 对照、第二 consumer、同进程及跨进程分别留证；通过完整要求审计后再 review、合并、发布不可变 tag。优先复用 target，保留程序/日志/截图和 WIP。

新宿主的状态输入、回调与焦点责任见[短接入说明](rust-agent-ui-host-integration.md)。
[审计快照](rust-agent-ui-delivery-status.json)记录源码、依赖、公开 Issue 和证据来源；它是未完成项清单，不是全量验收凭证。

## 证据入口

| 范围 | 记录与适用身份 |
| --- | --- |
| 当前验证 | [三端及原生续验](rust-agent-ui-current-validation-20261023.md)：f44 原生冻结程序与两处验收脚本修正后的离屏程序分别记录；不包含完整 Native 0.9.3 生命周期。 |
| 来源帧工厂 | [来源适配与契约修正](rust-agent-ui-thinking-source-20261023.md)：原归一化链、缓存淘汰/迟到修复、Run/只读 Pane 工厂；整体控件接线仍开放。 |
| 思考模式 | [受控 OutputMessage](rust-agent-ui-thinking-output-20261023.md) 已接线；[共享策略基线](rust-agent-ui-thinking-policy-20261023.md) 保留。[整帧思考输入](rust-agent-ui-thinking-parent-20261023.md)已接线；业务布局和最终三端验收仍开放。 |
| 当前工具共享状态 | [工具行标题/计数](rust-agent-ui-tool-header-20261023.md)、[URL 预览](rust-agent-ui-tool-url-20261023.md)、[连续分组](rust-agent-ui-tool-run-20261023.md)。同标签语义色等待 #25，完整工具视觉未完成。 |
| 较早原生证据 | [原生续验](rust-agent-ui-native-resumed-20261023.md)：7c531a 冻结程序的本地专项；保留作历史记录。 |
| 较早真实故障证据 | [反馈恢复](rust-agent-ui-feedback-recovery-20261022.md)：4097e4f 程序；当前成功路径不代替该故障路径。 |
| 严格失败 | [限高详情](rust-agent-ui-bounded-details-20261022.md)：#53 不豁免、不放宽断言，标准 RunPort 仍用原文档流。 |

详细阶段记录保留在[接入契约](rust-agent-ui-contract.md)及各专题，避免把旧阶段计数当作当前全量通过证明。

[思考载体与排版](rust-agent-ui-reasoning-carrier-20261023.md)：当前根 278、UI 588 项通过，1 项已有忽略；严格检查与本地 fixture 原生阅读/软件拼音通过，完整布局及真实三端验收仍开放。

[原始位置与展示顺序](rust-agent-ui-display-order-20261023.md)：当前根 281、UI 589 项通过，原生阅读通过；系统拼音首次失败、同程序复跑通过，稳定验收未证明，已补充 TekesUI #41。

[分区与 live 状态](rust-agent-ui-turn-planes-20261023.md)：共享 presentation 负责原 live-owner 规则与分区事实，live Working 行已接原 planner；根 285、UI 590 项通过。当前程序的真实三端原生 stream、stream-reading 和公共分页 profile 通过；物理 plane 布局/settled disclosure、原生分页按钮、稳定系统 IME 与 ProcessContainer 仍未完成。
