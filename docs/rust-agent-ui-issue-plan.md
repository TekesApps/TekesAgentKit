# Rust Agent UI 的 Issue 驱动交付计划

日期：2026-10-24。基线：`1ef2791`，PR #10。完整目标保持不变；本计划将既有实现与剩余验收映射为工作项，不能把拆票视为交付完成。

## 执行规则

- GitHub Issue 负责需求、优先级、依赖、状态与关闭标准；本文件是需求覆盖索引，专题文档是设计/证据，不为每次验证另开 Issue。
- 单个 Issue 先比较 ClientKit 对应 Views 与 Tekes 宿主、确认范围，再实施；主会话协调，复杂项交独立子会话，按需使用 subagent，小改动可直接完成。
- AgentKit 拥有 Rust 业务组件/presentation；TekesUI 拥有通用组件、输入、焦点、IME、滚动、渲染和进程能力。仅消费公开精确版本，缺口复用上游票，不改 SDK/Swift/Kernel。
- 状态分开记录：已实现待合并、部分实现、可推进、等待上游、等待桌面验收、已验收关闭。OPEN 不等于没写代码；已勾实现不等于完整验收。
- 每项完成回填实现提交、分层验证及未验证项、证据身份和 PR；设计变更同步专题文档。关闭前必须满足该 Issue 的全部验收，不能仅据测试计数。
- 历史验收保留原程序身份；重验证不是创建新 target/新应用身份的理由。桌面验收仍暂不运行。

## 总追踪与需求覆盖

总追踪：[AgentKit #14](https://github.com/TekesApps/TekesAgentKit/issues/14)；既有修复 #12/#13 已实施待合并，归 #23/#18，不重复开票。历史 headless SEP、session pointer、历史/只读文件及 presentation 已做内容归 #15/#27 的证据基线，不因本次建票而宣称重新验收。

| 原目标 | Issue |
| --- | --- |
| 依赖、契约、命名、公开API、demo/文档 | #15、#28 |
| Composer/草稿/发送取消/IME | #16、#27 |
| Transcript历史/推理/工具/富内容/来源 | #17、#19 |
| 输入/footnote右、机器/footnote左，SDK样式 | #17、#20、#21 |
| 分页/reveal/追加阅读/重连锚点 | #18、#27 |
| Indicators与RunPort | #21 |
| Sidebar导航/活动/置顶/共享 | #22 |
| SidePanel任务/子代理/SideChat/Summary | #23、#24、#25；已有#12/#13 |
| 三个ProcessContainer同/跨进程 | #26、#27 |
| Native/Pi/DSH、第二consumer、原生/系统IME/AX | #15、#27 |
| review、CI、merge/tag与磁盘/WIP | #27、#28 |

## 当前主/子会话并行安排

见[文件范围、负责人及串行集成门槛](rust-agent-ui-parallel-work-20261024.md)。本批并行推进 #22 多端 Sidebar、#23 Task/child 契约核对、#25 Summary 来源动作；主会话集中处理 CI、公共接口、构建与回填。#17/#21 共享组合链暂不同时写入；SDK 依赖和桌面验收仍按具体子项等待。

## 执行顺序

1. 可先推进 #15 的API/文档/依赖审计、#19的公开producer契约、#27的验收矩阵；这些准备不要求启动桌面。
2. 功能项按Views差异开发，依赖仅阻塞适用子项，不能把整个目标都归因于三个SDK缺口。
3. #18/#20/#26分别消费#53/#47/#48的固定公开交付；不因Issue状态变化就声称消费者已通过。
4. 所有完整目标通过#27验收后由#28负责review/CI/合并/阶段tag。其间分项收据不转移程序身份。

## 工作项与现有证据

### [#15](https://github.com/TekesApps/TekesAgentKit/issues/15) — [P1][API] 固定公开依赖、宿主接入契约与第二 consumer

当前：非 SDK 审计与独立 consumer 已完成；正式 UI 发行 pin 与容器契约待交付。

依赖/关联：[TekesUI #64](https://github.com/TekesApps/TekesUI/issues/64)。上游关联按具体子项适用。

已做：

- [x] Rust-only 根契约/SessionEndpointRegistry、RunScope 和 presentation 已建立；已发布基线 v0.3.8，UI 增量仍未发布。
- [x] 三 SEP 已有 pointer/locator、历史恢复、只读文件和能力矩阵；本地与远端 cache 策略独立。
- [x] 已提供 RunPort、Sidebar、SidePanel、显式宿主回调、第二公开 consumer 和 UI 边界检查。

待做：

- [x] 逐项核对公开类型、RunView→RunPort 命名、组件所有权与状态/事件输入。
- [ ] 为最终接入 revision 固定 AgentKit/TekesUI 闭包，核对安装、路径、模型/附件/设置等宿主职责。
- [x] 修正过期状态说明；将文档示例与真实公开 API 对照，保留显式开发依赖与正式精确 pin 的区别。

验收：

- [x] 干净外部 consumer 仅使用公开 API，构造三个业务组件；不依赖 RunSurface/ComponentDemo。
- [x] 文档列明版本、所有权、状态输入、事件回调、焦点/IME/滚动责任及迁移。
- [ ] 共享包+Rust demo+第二 consumer 适用检查通过；没有 Swift、私有 SDK 或新状态机。

证据：[session-endpoints.md](session-endpoints.md)、[rust-agent-ui-host-integration.md](rust-agent-ui-host-integration.md)、[rust-agent-ui-contract.md](rust-agent-ui-contract.md)。

本轮推进：本轮实现 `8335231`：公开构造与宿主状态/回调审计、独立 package 实际构造/挂载三组件、依赖/源码身份核验已完成。详见[契约审计](rust-agent-ui-api-contract-audit-20261024.md)。不代表正式 UI 发布或原生/ProcessContainer 通过。

本次 CI 复核：`32986d4` 的 PR/push 两套新增独立 consumer 均成功；UI 测试、格式和 Clippy 成功，整体仍在 #53 严格阅读失败。见[CI 收据](rust-agent-ui-ci-review-20261024.md)。

### [#16](https://github.com/TekesApps/TekesAgentKit/issues/16) — [P1][Composer] 输入面、草稿与发送取消对齐 ClientKit

当前：部分已实现；待补齐/验收。

依赖/关联：#15、[TekesUI #30](https://github.com/TekesApps/TekesUI/issues/30)、[TekesUI #48](https://github.com/TekesApps/TekesUI/issues/48)。上游关联按具体子项适用。

已做：

- [x] 使用原 TekesUI editor；已接发送/取消、模型/附件/上下文/notice 与受控选择。
- [x] 已有目的地草稿隔离、准备票据、迟到回调门禁和 no-auto-retry；Send/Stop preserving_focus 已消费。
- [x] Native/Pi SideChat 原 preedit/commit 合成门禁已验证，不等于系统物理 IME。

待做：

- [x] 逐项核对 Composer Views 层次、可见性、上下文和设置回调；不得把宿主能力猜成可用。
- [ ] 补完整同内容布局/焦点路径和系统中文组字验收；应用样式仅延展 SDK tokens。

验收：

- [ ] 组字 Enter 不发送，commit 后一次显式发送；取消遵循原请求身份。
- [ ] A/B 会话的文本、附件、设置草稿隔离；准备期间导航/关闭不会投递旧目标。
- [ ] 背景回传/按钮/切换/重连不误夺焦点或重复提交；组件和第二 consumer 有回归，桌面项留给 acceptance。

证据：[rust-agent-ui-primary-focus-child-runs-20261023.md](rust-agent-ui-primary-focus-child-runs-20261023.md)、[rust-agent-ui-sidechat-original-input-20261023.md](rust-agent-ui-sidechat-original-input-20261023.md)、[rust-agent-ui-composer-visibility-20261021.md](rust-agent-ui-composer-visibility-20261021.md)、[rust-agent-ui-context-preflight-20261020.md](rust-agent-ui-context-preflight-20261020.md)。

本次推进：[Composer 核对](rust-agent-ui-composer-issue16-audit-20261024.md)未发现确定实现缺陷，新增带正向控制的旧回调/能力撤销回归；3 个套件共 16 项通过，严格 Clippy/格式通过。实际宿主附件 materialize、完整布局/焦点、系统 IME/AX 尚未验收。

### [#17](https://github.com/TekesApps/TekesAgentKit/issues/17) — [P1][Transcript] 完整内容、思考工具分区与富内容对齐

当前：部分已实现；待补齐/验收。

依赖/关联：#19、[TekesUI #25](https://github.com/TekesApps/TekesUI/issues/25)、[TekesUI #54](https://github.com/TekesApps/TekesUI/issues/54)。上游关联按具体子项适用。

已做：

- [x] 已建立 InputMessage/OutputMessage、canonical 消息/工具身份、None/Compact、Activity/分段折叠、旁白载体和混合工具五类卡片。
- [x] 消息内与独立工具共享业务投影；普通完成默认折叠、异常展开及控件保留已有回归。
- [x] 三端混合工具离屏已有分项收据；richContent/completeTranscript 仍 false。

待做：

- [ ] 按 ClientKit 对应 Views 核对输入/输出/工具顺序、code header、完整连续分组与当前全组合。
- [ ] 消费公开富内容/文本映射/分隔符；补选中、复制、链接、代码与裁剪，不复制 renderer。
- [ ] 真实报告与独立旁白来源另由 sources 任务提供，不从正文或 meta 猜造。

验收：

- [ ] 同一内容在历史/流式/结算/恢复保持稳定身份与正确显示顺序，不重复消息或工具。
- [ ] 富内容及复制语义、独立/嵌入工具、折叠/展开与布局在窄宽/字号/主题下有对照证据。
- [ ] 静态与组件/第二 consumer/真实协议/原生证据分开；未完成项不得改 complete 标志为 true。

证据：[rust-agent-ui-physical-planes-20261023.md](rust-agent-ui-physical-planes-20261023.md)、[rust-agent-ui-physical-tool-fragments-20261023.md](rust-agent-ui-physical-tool-fragments-20261023.md)、[rust-agent-ui-mixed-tools-20261023.md](rust-agent-ui-mixed-tools-20261023.md)、[rust-agent-ui-narration-frame-reading-20261023.md](rust-agent-ui-narration-frame-reading-20261023.md)。

### [#18](https://github.com/TekesApps/TekesAgentKit/issues/18) — [P0][Reading] 分页、reveal、嵌套阅读与恢复锚点闭环

当前：等待公开 SDK；严格 CI 失败已复现。

依赖/关联：#13、[TekesUI #53](https://github.com/TekesApps/TekesUI/issues/53)、[TekesUI #42](https://github.com/TekesApps/TekesUI/issues/42)。上游关联按具体子项适用。

已做：

- [x] 已消费公开 #22/#26/#28 的锚点/preflight/reveal，标准文档流不使用像素估算。
- [x] 已有前插/重排/整帧事务、只读有界分页/epoch/错误保留，以及 #13 实例身份。
- [x] 当前严格门禁实证失败：内层向上读后父容器仍跟随追加，详情移出视口。

待做：

- [ ] 等待并消费 #53 的固定公开修复；保留严格失败，禁止改布局或放宽断言。
- [ ] 补新建 Host 的持久 checkpoint；分别验证主历史、工具详情与有界 child 的读取/分页。

验收：

- [ ] 严格 bounded_detail_reading accepted=true，父容器在内层向上读后不再被追加拉走。
- [ ] 主历史/只读历史按钮实际前插后保持原锚点；稳定 ID reveal 不夺编辑焦点。
- [ ] 显式回到底部后恢复跟随；重连/旧页/错误返回不污染当前 frame；新 Host 恢复按公开 checkpoint。

证据：[rust-agent-ui-bounded-details-20261022.md](rust-agent-ui-bounded-details-20261022.md)、[rust-agent-ui-native-bounded-20261023.md](rust-agent-ui-native-bounded-20261023.md)、[rust-agent-ui-reveal-current-20261022.md](rust-agent-ui-reveal-current-20261022.md)、[rust-agent-ui-source-authority.md](rust-agent-ui-source-authority.md)。

### [#19](https://github.com/TekesApps/TekesAgentKit/issues/19) — [P1][Sources] 明确工具报告与独立旁白 fallback 的公开来源

当前：公开来源审计完成；真实 report/group fallback producer 未确认，集成仍待来源。

依赖/关联：先确认公开 producer，无已知 SDK 硬依赖。上游关联按具体子项适用。

已做：

- [x] 已有 ToolHeaderAdapter、报告/计数选择和 narration fallback 纯规则及显式输入。
- [x] 标准 RunPort 的公开 new(...).with_transcript(..., labels) 已能消费宿主 adapter，无需重复 API。
- [x] 当前 SEP result_meta 是 opaque；既有测试拒绝将 didDo 当作权威报告。

待做：

- [ ] 针对每个 harness/宿主确认可核验的报告与 group narration producer、身份和更新语义。
- [ ] 若公开 producer 缺失，记录具体契约缺口/依赖；仅在明确来源后接入原投影与组件。
- [ ] 补真实协议来源 fixture 和回传/恢复证据；不以手造字符串或显示正文代替来源。

验收：

- [ ] 报告绑定原 scope/tool/revision，迟到和跨源记录被拒绝；无报告保留合法 fallback。
- [ ] 独立旁白只消费有权威来源的 group-text；严格前缀/结算/carrier 选择按 ClientKit，不重复正文。
- [ ] 每项标明支持/不支持的实际 harness；缺公开数据时保留阻塞，不猜私有数据库或 stdout。

证据：[rust-agent-ui-tool-header-20261023.md](rust-agent-ui-tool-header-20261023.md)、[rust-agent-ui-narration-20261023.md](rust-agent-ui-narration-20261023.md)、[rust-agent-ui-thinking-source-20261023.md](rust-agent-ui-thinking-source-20261023.md)。

本轮推进：本轮实现 `29c2f37`：27个来源文件逐哈希核对，26项策略与3项公开 consumer 回归通过。详见[公开来源矩阵](rust-agent-ui-public-sources-20261024.md)。三端 report/group fallback 为 unknown/no public producer；不猜造真实 fixture。

### [#20](https://github.com/TekesApps/TekesAgentKit/issues/20) — [P1][Footnote] 紧凑动作、目标与评价信息的单行排版

当前：数据链路已有；紧凑视觉等待公开 SDK。

依赖/关联：[TekesUI #47](https://github.com/TekesApps/TekesUI/issues/47)、[TekesUI #49](https://github.com/TekesApps/TekesUI/issues/49)。上游关联按具体子项适用。

已做：

- [x] 输入气泡与 input aside 右对齐，机器内容与 output aside 左对齐；共同列和单行原则已有 consumer。
- [x] 目标数据/清除/重连及反馈初读/CAS/错误恢复已有三端分项验证。
- [x] 真实 Pi 窄列目标 labelVisible=false 已保留严格失败；文字 fallback 不算完整视觉。

待做：

- [ ] 消费公开原 glyph/紧凑动作服务，保留完整 AX name 与原按钮焦点/selected/disabled。
- [ ] 按 ClientKit 单行 controlSlot+剩余信息布局完成复制/评价/分叉/时间/目标/状态可见性。

验收：

- [ ] 固定窄列真实目标标签、时间和所需状态可见，不隐藏业务动作、不改多行、不用假字符图标。
- [ ] 输入及其 footnote 始终右，机器输出及 footnote 始终左；主题/字号/浅深色跟随 SDK。
- [ ] 反馈载入/失败/恢复和 CAS 身份正确；复制/评价等动作一次触发且保留原 editor 焦点。

证据：[rust-agent-ui-real-goal-footnote-20261022.md](rust-agent-ui-real-goal-footnote-20261022.md)、[rust-agent-ui-feedback-recovery-20261022.md](rust-agent-ui-feedback-recovery-20261022.md)、[rust-agent-ui-goal-footnote-20261022.md](rust-agent-ui-goal-footnote-20261022.md)。

### [#21](https://github.com/TekesApps/TekesAgentKit/issues/21) — [P1][RunPort] 组合 Composer/Transcript/Indicators 并完成 Views 对照

当前：部分已实现；待补齐/验收。

依赖/关联：#16、#17、#18、#20、[TekesUI #46](https://github.com/TekesApps/TekesUI/issues/46)、[TekesUI #5](https://github.com/TekesApps/TekesUI/issues/5)。上游关联按具体子项适用。

已做：

- [x] 正式 RunPort 已组合三组件与 preflight/稳定回合导航，第二 consumer 不依赖测试宿主。
- [x] 连接/运行/等待/失败/用量/TurnRail/Jump 已接原 presentation；completeRunPort 仍 false。

待做：

- [ ] 逐项对照 ClientKit 完整层次、selection/chrome、hover、回合导航和可达性。
- [ ] 补同内容/同尺寸/同主题/同 scale 的最终布局记录，测试宿主 RunSurface 不作为标准组件。

验收：

- [ ] Composer/Transcript/Indicators 输入输出清晰，宿主不 fork 状态机。
- [ ] rail/跳转/状态数据绑定原回合；缺省 usage 不造数；reveal 不夺焦点。
- [ ] 布局对照逐项列差异，全部实现与适用验收通过才标 completeRunPort。

证据：[rust-agent-ui-run-port-20261022.md](rust-agent-ui-run-port-20261022.md)、[rust-agent-ui-turn-rail-20261021.md](rust-agent-ui-turn-rail-20261021.md)、[rust-agent-ui-run-preflight-20261021.md](rust-agent-ui-run-preflight-20261021.md)、[rust-agent-ui-delivery-status.md](rust-agent-ui-delivery-status.md)。

### [#22](https://github.com/TekesApps/TekesAgentKit/issues/22) — [P1][Sidebar] 会话导航、活动、置顶和共享操作完整验收

当前：部分已实现；待补齐/验收。

依赖/关联：[TekesUI #44](https://github.com/TekesApps/TekesUI/issues/44)。上游关联按具体子项适用。

已做：

- [x] 已有目录/搜索/分组/分页、恢复与选中补齐、稳定 action value/reveal。
- [x] 活动状态、置顶/偏好/共享 CAS 及捕获来源、旧请求门禁已有回归。

待做：

- [ ] 消费非当前行上下文菜单公开契约，核对原生/AX 操作及窄宽布局。
- [ ] 复验跨端点、过滤分页和导航后操作的来源归属；宿主只负责导航。

验收：

- [ ] 非当前会话右键不错误改变目标；pin/share/rename/archive 作用于原捕获 item。
- [ ] 搜索/分页/恢复/切换端点后选中项可见，无重复、漏项或旧游标写入。
- [ ] 活动状态不猜运行中；菜单/键盘/AX 语义与稳定ID在同进程和后续容器验收中一致。

证据：[rust-agent-ui-sidebar-navigation-20261021.md](rust-agent-ui-sidebar-navigation-20261021.md)、[rust-agent-ui-sidebar-paging-20261021.md](rust-agent-ui-sidebar-paging-20261021.md)、[rust-agent-ui-sidebar-preferences-20261021.md](rust-agent-ui-sidebar-preferences-20261021.md)、[rust-agent-ui-sidebar-session-menu-20261022.md](rust-agent-ui-sidebar-session-menu-20261022.md)。

本次交付：[多 endpoint 聚合](rust-agent-ui-sidebar-multi-endpoint-20261024.md) `6c700e9` 完成可复用 binding 和 5 项新回归，支持完整 address 的 selected-read/action 原端点路由、旧 owner 拒绝和局部终态故障保留。后续 `4967d15` 已接入 demo 宿主并补齐旧 owner 隔离、按 endpoint 路由与非匹配 SideChat 禁用；跨端全局搜索、真实多 harness 配置与过滤分页整体组合、终态 observer 同实例恢复仍未完成。详见[三子会话本批收据](rust-agent-ui-parallel-results-20261024.md)，不据 binding 测试关闭 Issue。

### [#23](https://github.com/TekesApps/TekesAgentKit/issues/23) — [P1][SidePanel] Task/子代理 tabs、pointer 与有界历史闭环

当前：部分已实现；待补齐/验收。

依赖/关联：#12、#13、#18。上游关联按具体子项适用。

已做：

- [x] 已有受控 SidePanelInput/Navigation、动态 Task pointer、同目标 canonical controller 和独立 history lease。
- [x] Native 真实有界子历史/单消息分页/重连/父草稿隔离；Pi/DSH 可继续子 Run 已有离屏分项记录。
- [x] 现有 #12 继承父权限、#13 只读实例身份与 display-only 门禁已实施待合并。

待做：

- [ ] 核对 Task/子代理 tab 的 loading/missing/error/ready、权威 pointer 替换和父导航完整组合。
- [ ] 补同一交付程序的三端实际来源与完整当前窗口/chrome；不从工具调用猜造独立会话。

验收：

- [ ] A→B→A/父关闭/异步迟到不恢复旧权限；同目标共享草稿，子 pane 关闭不丢父草稿。
- [ ] 只读有界 pane 无 Composer 且不能发送；合法来源可刷新/分页，源切换不串帧。
- [ ] 真实 Task pointer 与家族导航来源可追溯；所有状态/空态和 tab 切换均有明确验收。

证据：[rust-agent-ui-sidepanel-contract-audit-20261023.md](rust-agent-ui-sidepanel-contract-audit-20261023.md)、[rust-agent-ui-task-shared-destination-20261023.md](rust-agent-ui-task-shared-destination-20261023.md)、[rust-agent-ui-native-bounded-20261023.md](rust-agent-ui-native-bounded-20261023.md)、[rust-agent-ui-source-authority.md](rust-agent-ui-source-authority.md)。

本次推进：`8b5d45b` 的[SidePanel 定向审计](rust-agent-ui-sidepanel-issue23-audit-20261024.md)未发现确定产品缺陷；四套 demo 测试 41 项通过。当前窗口/chrome、明确列出的状态组合仍待验收。

### [#24](https://github.com/TekesApps/TekesAgentKit/issues/24) — [P1][SideChat] 临时分支生命周期、重开与能力门禁

当前：部分已实现；待补齐/验收。

依赖/关联：#12、#16、#23。上游关联按具体子项适用。

已做：

- [x] Native/Pi 真实创建/发送/切换草稿/同实例重开/重连不重复/确认丢弃离屏闭环已通过。
- [x] 同目标 controller 复用，Closing/Unknown/absence 和原编辑器合成 preedit/commit 有回归。
- [x] DSH 公开 runtime 无临时 discard，当前禁用是正确门禁，不算支持。

待做：

- [ ] 补当前交付程序的系统中文输入、菜单/关闭确认与完整原生生命周期。
- [ ] 若 DSH 要支持 SideChat，先确认公开 temporary fork/discard 契约与上游交付；否则正式记录 Unsupported，不要求伪实现。

验收：

- [ ] 重开保留原草稿/附件/准备状态；丢弃须原目标确认，Unknown 不重试 mutation。
- [ ] Native/Pi 真实端点一次发送、原临时子分支可正确关闭/恢复，父草稿不变。
- [ ] DSH 若无公开能力，UI 与API 均明确不可用；若能力新增需真实端到端证明后启用。

证据：[rust-agent-ui-real-sidechat-20261023.md](rust-agent-ui-real-sidechat-20261023.md)、[rust-agent-ui-sidechat-shared-destination-20261023.md](rust-agent-ui-sidechat-shared-destination-20261023.md)、[rust-agent-ui-sidechat-original-input-20261023.md](rust-agent-ui-sidechat-original-input-20261023.md)。

### [#25](https://github.com/TekesApps/TekesAgentKit/issues/25) — [P1][Summary] sections、用量、metadata 和来源动作对齐

当前：部分已实现；待补齐/验收。

依赖/关联：[TekesUI #47](https://github.com/TekesApps/TekesUI/issues/47)、[TekesUI #25](https://github.com/TekesApps/TekesUI/issues/25)、[TekesUI #30](https://github.com/TekesApps/TekesUI/issues/30)、[TekesUI #38](https://github.com/TekesApps/TekesUI/issues/38)、[TekesUI #46](https://github.com/TekesApps/TekesUI/issues/46)。上游关联按具体子项适用。

已做：

- [x] 已有受控 SummaryInput/Context、sections/rows、展开/用量/metadata/宿主 slots 与 source-owned actions。
- [x] 行布局/缩进/label/context/child 状态等有专题收据和第二 consumer。

待做：

- [ ] 消费状态附件/图标、语义可选内容；核对 focus/hover/selection 与最终同内容布局。
- [ ] 所有宿主自定义内容通过正式 slots/callbacks，不另造 AgentKit 基础控件。

验收：

- [ ] section/行状态、usage 与 metadata 和原 presentation 一致，缺字段不补假值。
- [ ] 原来源动作和异步结果不串 session；关闭/切换后的请求被拒绝。
- [ ] 窄宽/字号/主题下行与图标/label 语义正确；完整当前原生与可达性单独有收据。

证据：[rust-agent-ui-summary-context-20261022.md](rust-agent-ui-summary-context-20261022.md)、[rust-agent-ui-summary-sections-20261021.md](rust-agent-ui-summary-sections-20261021.md)、[rust-agent-ui-summary-row-layout-20261022.md](rust-agent-ui-summary-row-layout-20261022.md)、[rust-agent-ui-summary-child-status-20261022.md](rust-agent-ui-summary-child-status-20261022.md)。

本次推进：`9a62950` 修复 Summary 子代理 pointer 的已知 session 授权门禁，阻止未知/歧义/父 UUID 重定向；presentation 225、UI 634/1 原有忽略通过，当前程序 Pi/DSH 原 Summary→子 Run 非桌面闭环通过。见[Summary 审计](rust-agent-ui-summary-issue25-audit-20261024.md)及[统一收据](rust-agent-ui-parallel-results-20261024.md)；不等于视觉/Native/系统 IME 验收。

### [#26](https://github.com/TekesApps/TekesAgentKit/issues/26) — [P0][ProcessContainer] 三个业务容器的同进程与跨进程宿主接入

当前：等待完整交互式 ProcessContainer 公开交付。

依赖/关联：#15、[TekesUI #48](https://github.com/TekesApps/TekesUI/issues/48)。上游关联按具体子项适用。

已做：

- [x] Sidebar/RunPort/SidePanel 目前可由同进程 ContentHost 组合；没有正式 ProcessContainer 适配。
- [x] 已检查旧公开 module-runtime/app-protocol：远端 keyboard/IME/scroll/AX 能力未满足；不把 ContentHost 叫作已验收容器。

待做：

- [ ] 等待 #48 完整交互契约与固定公开版本，再接入三个业务容器。
- [ ] 实现消费侧状态/事件边界、挂载/卸载、进程恢复/来源身份；不补私有传输或复制焦点/IME引擎。

验收：

- [ ] 同进程与跨进程分别记录真实运行结果、精确版本和宿主身份。
- [ ] 三个容器均支持原输入/焦点/IME/滚动/AX/唤醒及隔离；进程退出或重连不重复发送。
- [ ] 独立公开 consumer 可复现，无 active sibling override，无私有 SDK 依赖。

证据：[rust-agent-ui-host-integration.md](rust-agent-ui-host-integration.md)、[rust-agent-ui-delivery-status.md](rust-agent-ui-delivery-status.md)。

### [#27](https://github.com/TekesApps/TekesAgentKit/issues/27) — [P0][Acceptance] 固定宿主的三端、IME、阅读与布局验收矩阵

当前：非桌面矩阵与本轮三端离屏已完成；系统 IME、窗口、阅读锚点及容器验收待补。

依赖/关联：#21、#22、#23、#24、#25、#26。上游关联按具体子项适用。

已做：

- [x] Native/Pi/DSH 独立 SEP、session pointer/历史/数据回传与 headless 路径已实现并有 v0.3.8 基线。
- [x] 已有多个冻结程序的真实协议/离屏/部分原生分项收据，范围不能互相转移。
- [x] 固定宿主路径/bundle ID/签名策略和外部截图入口已交付；未实际完成当前宿主安装签名验收。

待做：

- [ ] 建立逐需求×harness×同/跨进程×验证层级矩阵，先复核组件/第二 consumer/离屏，再做必要原生。
- [ ] 继续遵守用户“桌面验收先不跑”；获安排后用固定宿主与已授权固定截图工具，历史程序仅归档。
- [ ] 同一交付程序补系统中文组字/草稿/发送取消/stream/向上阅读/原生分页/reconnect 及配对布局/AX。

验收：

- [ ] 每条记录程序/来源/版本/命令/结果/证据，区分 unsupported、blocked、failed 与 passed。
- [ ] 组字不误发送，草稿隔离，阅读追加不打断，分页保锚，重连不重发；不是只用合成事件证明系统IME。
- [ ] 保留源码/WIP/固定快照/日志/截图/程序，复用共享 target；清缓存先核对进程/依赖并报告实际释放。

证据：[session-endpoints.md](session-endpoints.md)、[rust-agent-ui-non-sdk-review-20261023.md](rust-agent-ui-non-sdk-review-20261023.md)、[rust-agent-ui-native-toolchain.md](rust-agent-ui-native-toolchain.md)、[rust-agent-ui-offscreen-coverage-20261023.md](rust-agent-ui-offscreen-coverage-20261023.md)。

本轮推进：见[720格分层矩阵](rust-agent-ui-acceptance-matrix-20261024.md)及[本轮非桌面实跑](rust-agent-ui-non-desktop-results-20261024.md)。程序/源码身份独立保留，不把矩阵检查成功或离屏结果当作完整验收。

### [#28](https://github.com/TekesApps/TekesAgentKit/issues/28) — [P0][Delivery] 完整要求审计、review、CI、合并与阶段 tag

当前：等待功能/验收完成与严格 CI/review。

依赖/关联：#15、#27、#18。上游关联按具体子项适用。

已做：

- [x] PR #10 draft，最新代码 1ef2791；#12/#13 已实施但尚未合并。
- [x] 1ef2791 两套 CI 的 headless/UI测试/格式/Clippy 通过，严格 #53 reading 失败，两份JSON已归档。
- [x] 设计/迁移/接入和专题证据已有；当前 UI 0.0.0/publish=false，不是 v0.3.8 中已发布 UI。

待做：

- [ ] 全部工作项逐项核对完整目标，不将分项通过等同 full Views 完成。
- [ ] 所有必要 CI 与独立 review 通过后，按发布契约更新版本/文档并合并、推送不可变阶段 tag。
- [ ] 关闭完成的业务Issue；仍后置的能力逐项留票并清楚说明阶段范围，不更改本完整目标定义。

验收：

- [ ] 需求覆盖表无未归属项，证据适用当前交付 revision，必要原生/同跨进程/三端验收齐全。
- [ ] 严格失败不 skip、不放宽；review发现已处理；正式依赖是已发布精确pin。
- [ ] merge commit/tag/远端发布与 consumer 接入说明可核验，Issue回填实现/验证/未验收项/PR。

证据：[rust-agent-ui-delivery-status.md](rust-agent-ui-delivery-status.md)、[rust-agent-ui-non-sdk-review-20261023.md](rust-agent-ui-non-sdk-review-20261023.md)、[rust-agent-ui-source-authority.md](rust-agent-ui-source-authority.md)。


## 状态记录与关闭规则

#12/#13有原始实现/验证评论，新工作项的已勾列表是历史实现，不等于整个Issue已完成。14个新工作项保持OPEN，只有全部本项验收且实现已合并可核验后关闭。#28最后审计完整目标；PR #10当前不会自动关闭这些功能票。

新任务可使用 `.github/ISSUE_TEMPLATE/agent-ui.md`；机器索引为 `rust-agent-ui-issues.json`。本次仅梳理设计/Issue/工作流程，没有重编译、启动桌面、修改上游或产生新target。

## Sidebar 宿主续批交付

#22 `4967d15`、#23 `9d7c00d`、#25 `7bfdd61` 的本批实现、分层验证和未验收项见[统一收据](rust-agent-ui-sidebar-host-results-20261024.md)。UI637/0/1 原有忽略、最终宿主定向、公开 consumer 和严格检查通过；准确源码顺序以收据为准，未做桌面/真实多 harness 同窗验收，Issue 仍开放。
