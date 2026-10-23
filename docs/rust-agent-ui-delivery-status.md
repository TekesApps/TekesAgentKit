# Rust Agent UI：当前接入范围与交付状态

本页以已提交基线 `378c631` 与本轮主按钮焦点修复、真实可继续子 Run 验收增量为准，按用户完整目标保留验收范围。
当前可供实验宿主接入，**完整交付尚未达到**。组件存在、离屏通过或单个 Issue 关闭都不能单独证明完整 Views 对齐。
本阶段增量的证据见[三端离屏覆盖补验](rust-agent-ui-offscreen-coverage-20261023.md)与[固定验收工具链](rust-agent-ui-native-toolchain.md)；不替代本页原阶段的程序身份和原生结论。
最新本地全 UI 为 618 通过、1 项既有忽略，Python 74 通过，严格 Clippy/格式/边界通过；
当前源码和程序身份见[主按钮焦点与子 Run 收据](rust-agent-ui-primary-focus-child-runs-20261023.json)；此前[工具组阶段](rust-agent-ui-global-tool-runs-20261023.json)保留原身份。
任务/子代理/SideChat 的本轮源码核对见[SidePanel 契约](rust-agent-ui-sidepanel-contract-audit-20261023.md)。
Pi 后续验证采用[精确 1.4.0 候选](pi-1.4.0-compat-20261023.md)，保留 1.3.0 基线，旧离屏记录仍按原程序身份解释。
实际分区的基础实现见[普通回合折叠阶段](rust-agent-ui-physical-planes-20261023.md)：原 reasoning/工具与 final 正文分属实际阅读行，正常完成默认折叠、异常终止默认展开，原控件与本地选择保留。前一阶段组件/CoreText 与三端离屏验证见[frame 阅读范围](rust-agent-ui-narration-frame-reading-20261023.md)，本轮范围见[原工具组物理分区](rust-agent-ui-physical-tool-fragments-20261023.md)；当前程序未做原生验收，历史原生程序 phase 0 失败保留。
共享 [Activity 判定](rust-agent-ui-activity-policy-20261023.md) 与此前 [控制器分离](rust-agent-ui-output-controller-20261023.md) 保留原阶段身份。回合内追加输入的分段折叠已接入，见[输入分段验收](rust-agent-ui-activity-segments-20261023.md)；journal prompt role 已接入，见[来源与显示](rust-agent-ui-system-prompts-20261023.md)；完整富内容/配对布局等仍未完成。
原生、真实协议、离屏 GPU 和跨进程证据分别记录，不转移冻结程序身份。较早程序的独立记录见[历史续验](rust-agent-ui-current-validation-20261023.md)。

混合工具阶段已补[三端真实混合工具离屏闭环](rust-agent-ui-mixed-tools-20261023.md)：Native 0.9.3 UI-only、Pi 1.4.0、DSH 0.2.1-rc.2 均通过。增量 demo 4 项、Python 74 项、严格 demo Clippy/格式/边界通过；原全 UI 616/1 忽略是此前阶段计数，本轮不冒称重跑。固定宿主尚未实际安装/签名，未启动桌面或申请权限。

PR #10 的 `ea36946` 远端 CI：checks 成功；UI 在严格 nested-detail-reading 步骤失败，仍受 TekesUI #53 阻挡，不合并、不放宽门槛。

[真实 SideChat 离屏接入](rust-agent-ui-real-sidechat-20261023.md)已补：Native/Pi 的原创建、独立 RunPort、发送、切换草稿、重连与确认丢弃通过；DSH 因公开 runtime 缺少 discard，仅证明原不可用门禁，不宣称其 SideChat 可用。

`abfc1c2` 的 pull_request CI 已完成：checks 成功，UI 的格式、Clippy、workspace 测试通过，严格 nested-detail-reading 仍以原始 `accepted:false` 失败；未合并，未豁免 #53。

[Native 有界子历史当前程序补验](rust-agent-ui-native-bounded-20261023.md)已完成：Kernel 0.9.3 实际 task 子历史、原工具参数只读详情、单消息分页、保留行、重连和非空中文父草稿隔离通过。原读取探针已适配独立 ToolActivity/结构化详情与原后台读取回调；未修改组件或 SDK。现有 Native 协议驱动场景本轮也通过，区别于此前明确 UI-only 的记录。

[主按钮焦点与真实可继续子 Run](rust-agent-ui-primary-focus-child-runs-20261023.md)：Send/Stop 通过原 SDK `preserving_focus()` 保留编辑器；独立 consumer 和红/绿回归已验证。Pi/DSH 从原 Summary 进入子 Run，发送/取消、合成组字门禁、草稿切换、重连和旧控制权失效通过；DSH 同程序两次独立会话通过，当前 Native SideChat 另通过。Task 仍只消费权威行的真实 pointer，不从工具调用猜造独立会话。

## 依赖与运行身份

| 输入 | 当前固定值及限制 |
| --- | --- |
| 已发布 AgentKit | `v0.3.8`，根 workspace 版本同为 0.3.8；本分支包含尚未发布的共享增量，不能把 UI 当作未修改的 v0.3.8 消费者。 |
| UI packages | `ui/` 为独立 workspace，0.0.0 / publish=false；开发使用显式仓库内 path，固定整个 AgentKit revision。正式宿主须等待包含所需 API 的新不可变发布 pin。 |
| TekesUI | 公开 `50ec49f3446924fdbc66b3cff22cd8cfe150fe7c`，Cargo.lock 中 8 包；`ui/tekesui-source.json` 核验 243 输入，清单 SHA `3ad8aa98a567679b775e78b634f45888c9eb70ffd05708a4f781c0ce7264471c`。 |
| 原生运行时 | Pi 1.4.0 候选（1.3.0 基线保留）、DSH 0.2.1-rc.2；Native 0.9.3 本轮协议驱动场景及有界子历史离屏通过；此前 UI-only 记录范围不变。未证明整个 Kernel 所有能力或当前原生窗口验收。 |
| 宿主路径 | 当前 Sidebar / RunPort / SidePanel 使用同进程原 ContentHost；未按 ProcessContainer 同/跨进程契约验收。RunSurface/ComponentDemo 是测试宿主。 |

## 按完整目标核对

| 要求 | 当前实现/证据 | 距离完整交付还差什么 |
| --- | --- | --- |
| Composer、发送/取消、流式回传 | 原 TekesUI editor + RunBinding/RunController；归档/中断、草稿、请求身份、模型/附件等有组件与宿主用例。 | 历史冻结程序原生 phase 0 失败保留；当前程序尚未启动桌面验收，软件/物理 IME 的完整组合仍待；上下文高级入口按源规则逐项验收，缺少宿主能力时通过明确输入/回调接入。 |
| Transcript：历史/推理/工具/分页/阅读 | 混合 Content、原阅读事务、canonical 身份与只读子历史；公开 page/reveal 已消费，工具关闭边界、共享 row、报告/完整计数选择规则与五类卡片事实有回归。 | 独立 Input/Output/view 原字段与复制通道已接入；真实报告来源、连续分组的完整输入/原生验收、富内容、#54 分隔线和最终 code-header 样式未完成，`rich_content=false`；选中富内容 #25、嵌套阅读意图 #53、新建 Host 的持久 checkpoint #42、当前原生与配对布局。 |
| 实时思考模式 | 共享 None/Compact、来源绑定 Frame、独立 OutputMessage 与原归一化来源工厂已实现；Root Run/Transcript/Pane API 和第二 consumer 的 fixture 路径通过。Journal anchor 与 Pi ancestry 按实际 projector 定义，不使用 UI/buffer 下标作身份。 | TranscriptContent/RunPort/只读 pane 整帧输入及 prepared-plan 门禁已接线，demo 显式消费 Compact；reasoning carrier 与单详情/空白行已对齐；消息容器/独立工具的展示顺序、普通回合实际 reasoning/工具/正文分区和整段 Activity 折叠已接入；回合内追加输入分段已接入；journal initial prompt role 已接入；旁白选择/当前 prose 接线已接入；多记录 frame 的组件阅读锚点已通过，当前原生、独立 fallback 来源、富内容节点投影、mixed reasoning/tool 的当前原生全组合和配对布局仍待完成（三端离屏见专项收据）；裁剪/未知来源不能猜补。 |
| Input / output / footnote 排版 | 用户气泡/aside 右、机器内容/aside 左；共用列、单行、备注限宽与保留控件有独立 consumer。 | 真实 Pi 目标标签在窄列仍不可见，严格命令退出 1；#47 原 glyph/紧凑动作消费，完整时间/目标/状态可见性。 |
| Indicators 与回合导航 | 原连接/运行/等待/失败/用量事实、TurnRail/Jump 与原 SDK reveal，非像素估算。 | 完整 hover/selection/chrome、可达性和当前程序原生对照；#46/#5 等。不能把已有单行状态当作完整 rail 的证明。 |
| RunPort 组合与完整 Views 布局 | 正式 RunPort 组合 Composer/Transcript/Indicators，整帧 preflight/阅读发布；第二 consumer 无需 RunSurface。 | 完整 ClientKit 对应层次/selection/chrome 的逐项同内容对照；当前收据仍明确 `completeRunPort=false`。 |
| Sidebar 会话/活动/置顶/共享操作 | 受控目录/恢复/搜索/分组、稳定 menu value、原共享 action/CAS、分页/选中补齐和 reveal。 | 非当前行的原上下文菜单 #44、完整当前原生/AX验收；不把 compact toolbar 的缺口套到已经消费的普通 MenuContent 上。 |
| SidePanel Task/Subagent/SideChat/Summary tabs | 同一 SidePanelInput + SidePanelNavigation；动态 pointer、retained editor、关闭确认和迟到回包门禁有宿主/第二 consumer 用例。 | SideChat 家族菜单的本地协议演示已通过（phase 25；见[原生续验](rust-agent-ui-native-resumed-20261023.md)）；Native/Pi 真实 SideChat 离屏闭环已补，DSH 无 discard 能力；Pi/DSH 可继续子 Run 的真实组件组合已补；Task 的权威来源组合、最终原生、完整窗口 toolbar/Panel chrome 及 #48 仍待。 |
| Task/child 的可写与有界只读来源 | TaskPaneBinding、ChildPaneBinding、SideChatBinding 复用现有 presentation；有界历史无 Composer、不用 parent 正文代替。 | 每种 loading/missing/error/ready 的当前完整原生组合；Native 真实多消息有界路径与 UI-only 验收分别记录。 |
| Summary sections / rows / actions | 受控 SummaryInput/Context、展开/用量/metadata/宿主 slots、原 intent 与 source ownership。 | #47 状态附件/图标、#25 语义内容、#30/#38/#46 部分焦点/指示/hover组合与最终产品对照。 |
| 反馈初读/重连/CAS | 自动按来源/epoch读取，普通 aside 无常驻 Refresh；加载失败保留旧记录且不可写，明确刷新恢复。 | 本轮真实三端离屏组合验证见当前续验；真实故障恢复仍属 4097e4f 冻结程序，反馈的当前原生交互仍待验收。 |
| ProcessContainer 同/跨进程 | 当前没有正式适配；8 包闭包不含 module-runtime/app-protocol。 | #48 固定公开 SDK 中 `new_view` 仍声明无远端 keyboard/IME/AX，V1 的 keyboard/scroll/text_ime/accessibility 为 false；本轮复核公开 50ec49f 仍如此；需 SDK 交付后再接入三个业务容器并独立验收。 |
| 原生、中文组字、阅读/重连、第二 consumer | 有多个冻结程序的分项成功、失败和完整收据；组件/第二 consumer 在 CI 跑，未打开桌面。 | 同一交付程序的完整 Native/Pi/DSH + 当前系统 IME、最终配对布局；跨进程/VoiceOver 未证明。前一阶段 c90c9f3b 程序已通过三端 stream/stream-reading/public-page、owned 原生 Activity 点击/控件保留和一次系统软件拼音。public-page 不是原生分页按钮，SideChat 旧收据不转移到当前程序；物理键盘、稳定 IME、完整配对布局与跨进程未证明。 |
| review、CI、合并/tag | PR #10 draft；远端 baa2f55 的 checks x2 成功、UI x2 在严格 nested-detail step 失败。本阶段本地 UI 616 通过/1 既有忽略、Python 68 通过，严格 all-target Clippy/格式/边界通过。Pi 1.4 兼容增量和 310 根测试证据独立记录，不再声称根源码未变。 | 完整 review、全部要求与验收审计、新发布 pin；严格失败仍在，不能合并或打完成 tag。 |

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

[当前输入分段阶段](rust-agent-ui-activity-segments-20261023.md)：cb3b3aa3 冻结程序与源码、原生控件/阅读/软件组字及三端证据单独记录，完整 UI 交付仍未证明。

[当前 system prompt 阶段](rust-agent-ui-system-prompts-20261023.md)：6a3a49e7 冻结程序的来源、控件、三个 owned 原生专项、三端 native stream/reading/page 和一次系统软件拼音分别记录，完整 UI 交付仍未证明。

[当前旁白阶段](rust-agent-ui-narration-20261023.md)：0c3219f4 程序根 308、UI 604/1 忽略、严格 Clippy 与三端离屏通过；原生窗口未激活而失败，不转移此前程序的通过记录。多记录 frame 阅读容器是下一步业务工作。
