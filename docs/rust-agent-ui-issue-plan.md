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

本次新增：显式验收 feature 在原 RunController 发布边界捕获 Snapshot 与同次内存输入；真实 Pi 模型子代理经公开等待/childHistory/重连后，同帧 Summary 的 idle 来源及原 Summary 组件挂载已验证。没有显式 raw status 时不填造 completed；running/failure 转换、父草稿与完整 UI 验收未由本切片补齐。见[契约](rust-agent-ui-summary-source-witness-contract-20261024.md)和[结果](rust-agent-ui-summary-source-witness-results-20261024.md)。#25 仍开放。

Native后续切片：复用既有feature headless与已安装Kernel0.9.3，真实Task及公开bounded history/重连、同次Summary/Journal来源、原Summary被动行挂载通过；不授予独立Task pointer。见[Native收据](rust-agent-ui-native-summary-source-results-20261024.md)。同Task动态pointer、running/failure、完整UI/桌面等仍未验收。

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

## 多端 provider 搜索续批

#22 已补齐固定端集合 query/ABA/clear/drop/reconnect/retry、逐端原 inventory join、Sidebar 状态子组件、实际多 connector 配置与 demo 接线。最终 presentation234/0/0、UI653/0/1、第二公开 consumers 及双端成功/局部失败离屏通过；精确边界见[收据](rust-agent-ui-multi-search-results-20261024.md)。此前“跨端全局搜索未实现”描述仅适用于当时批次；本批不涵盖 Mirror FTS/pins、远端 cursor、真实多 harness、桌面或多端 SideChat。Issue 继续开放。

## 多端 SideChat 续批

#24 的非首端 SideChat 继续由主会话集成、新子会话负责 owner 证明与隔离测试。固定 canonical binding 按捕获 parent 路由，原端 pointer 与返回 Run owner 双重校验；每端单独清理面板，不影响另一端草稿、附件或 prepared ticket。设计和最终验证范围见[多端 SideChat 契约](rust-agent-ui-sidechat-multi-endpoint-20261024.md)。既有真实 Native/Pi 收据仍属于原程序；DSH 无公开 discard 不提升能力，桌面/IME/AX 仍未验收。

#22 `3d594a8` 补齐共享 binding 的显式同实例 observer 恢复，按端退旧代，保留原 subscriber、旧行和 running anchor；健康端 action/selected 凭据不受影响。最终 UI664/0/1、严格 Clippy、独立公开 API consumer 与普通 demo build 通过，见[恢复契约/收据](rust-agent-ui-sidebar-observer-recovery-20261024.md)。demo 用户重试入口仍待接线，真实故障/桌面验收未运行。


## Observer 用户重试入口续批

#22 `be4fc58` 已接通 coherent observer packet、默认控件状态 footer、来源/代数绑定的用户 Retry 和 demo 原端路由；同源重绑及健康端更新不清 pending，旧 raw ID 不复用。分层验证为 UI675/0/1、最终组件10/0/0、presentation240/0/0、严格 Clippy、普通 demo build 和独立 consumer，见[交付收据](rust-agent-ui-sidebar-observer-retry-results-20261024.md)。前文“用户入口未接线”仅指当时历史批次；真实窗口/断流/IME 与完整 Sidebar 验收仍开放。


## 查询成员关系续批

#22 `147642b` 对齐当前 ClientKit 会话 Sidebar 的共同 query/folder predicate，过滤未匹配 pins，保留 canonical 偏好/草稿及诊断，补齐 hasMore 空首页与 failed-ready 校验。UI677/0/1、严格 Clippy、普通 demo build 和独立 consumer 通过，见[收据](rust-agent-ui-sidebar-query-membership-results-20261024.md)。旧“独立 pins 不受筛选”解释已标成历史；本地名称 FTS、排名与真实窗口仍未完成，不关闭 Issue。

## 2026-10-24：#22/#23/#25 并行续批

主会话继续负责 #22 的真实本地名称 FTS/provider 并集、demo 索引来源和逐端就绪接线；独立子会话分别负责 #23 SidePanel 状态路径、#25 Summary 缺失字段与来源动作；另有独立只读 CI 与集成审查。全部 Cargo 验证由主会话串行执行，继续共享已有 target；桌面/截图/权限操作均不运行。

- #22：新增真实内存 SQLite FTS5 派生索引、canonical title 清除修正、persistent projector 与 Single/Multi demo 接线。精确依赖/缓存来源/空态语义见[查询契约](rust-agent-ui-sidebar-query-membership-20261024.md)。bm25 排序、公开远端 cursor、完整 main/ephemeral 分类事实和真实多 harness/窗口验收仍开放。
- #23：[状态路径续审](rust-agent-ui-sidepanel-followup-20261024.md)未发现确定实现缺陷，保留既有状态机；指明 native ChildOpened(Err) dispatch 的直接运行证据与完整窗口/三端来源尚缺，不以静态审查代替执行。
- #25：[Summary 续审](rust-agent-ui-summary-followup-20261024.md)修复 loaded Summary 无 usage projection 时隐藏整个用量区的问题，保留六条不可用值，区分已知零值和整个 Summary 不可用。来源动作与 slots 未发现确定缺陷。

本批实现提交与分层验证见 [693/241/92 结果收据](rust-agent-ui-name-fts-results-20261024.md)；这些子项完成不等于关闭各 Issue 或允许合并 PR #10。77a9ed8 CI 的独立 consumer 已通过，严格详情阅读仍失败，详见 [CI 复核](rust-agent-ui-ci-review-20261024.md)。

本批去重新增 [#29 会话分类事实](https://github.com/TekesApps/TekesAgentKit/issues/29)，关联 #22：Pi 已有显式分类事实可先补 Sidebar 规则；Native/DSH 的完整公开来源仍须确认。只在 Sidebar 投影/选中补读应用明确排除规则，不能删除 canonical 全局 child/history，也不以 parentSessionId 判断子代理。Kernel 实现仍在范围外。

### #29 已有明确事实的实施切片

`be6e50e` 已完成 Sidebar 列表与 selected 补读的同一显式 predicate：origin=subagent 或 ephemeral=true 排除，parent-only fork/未知分类保持兼容。canonical 全局 inventory/child/history 完整保留，旧 selected facts 被分类更新撤销；多端同 ID 和 aggregate/selected 路径有回归。UI695/0/1、严格UI Clippy/格式/边界、普通demo及复用独立组件consumer通过，精确记录见[分类契约](rust-agent-ui-sidebar-membership-20261024.md)与[机器收据](rust-agent-ui-sidebar-membership-results-20261024.json)。Native/DSH 分类producer与缺字段语义仍未证明，#29不关闭，桌面仍未运行。

### #29 固定 producer 与 adapter wire 核验

`36f0668` 新增 Native/DSH 真实 Rust adapter 的本地 wire 回归，未改生产适配器。Native固定公开v0.9.3 tag/API与精确Git对象核验，fork/ephemeral语义已证明；DSH两个允许版本的官方源码Git blob核验，在线/冷列表均透传origin/parent，SessionSummary不含ephemeral；Pi规范化producer亦记录在[能力矩阵](session-endpoints.md#sidebar-分类事实292026-10-24)。Native mux6/0/0、DSH14/0/0、严格root Clippy/格式/边界通过。这是源码与协议mock层，不是重新运行已发布harness或桌面；#29仍保留组合分页/真实运行身份等未验收项。

### #22 排名待实施契约

[全保留名称语料与查询排序](rust-agent-ui-sidebar-ranked-search-20261024.md)纠正了直接在可见行上加bm25的错误捷径，并核对最终pin_order/标题宿主comparator。实现先发布与库存同frame的retained-name语料，再让bindings/projector/demo实际消费并全局排序后分页；固定语料域不随query/目录/当前Run改变。根headless仍禁止SQLite。当前已开始来源层子任务，尚未交付排名，不重复建Issue。

### #22 全语料排名与 #23 错误分发的后续交付

`3aaee78` 已接通 retained-name 来源、Single/Multi 原始 packet、全局 bm25、demo sort 偏好与分页消费；修正搜索无限展开为 bounded page。此前“来源层进行中/排名未实施”只适用于原阶段。`adbb7f3` 补上 #23 native ChildOpened 错误当前/迟到分发回归，#25 复核没有新增独立实现缺陷。UI706/0/1、presentation+web258/0/0、Python92、严格检查、普通 demo 与独立排名 consumer 通过，准确边界见[本批收据](rust-agent-ui-sidebar-ranked-results-20261024.md)。新 head CI 独立跟踪，真实保留生命周期/远端 cursor/桌面等不宣称完成；原 Issue 和 #53 严格门槛保留。

### #22 / #29：活动 inventory knownness 与混合成员分页

已复现并修复 provider 命中隐藏但已知的 child/ephemeral 时误报 unresolved 的缺陷。复用同帧完整 activity_at，不把 retained archives 当 active，也不制造行/摘要/reveal。补齐混合 main/child/ephemeral/parent-only fork、跨端同ID、目录/搜索/分页/selected/reveal及canonical history 保留的实际 fixture-runtime 回归；统一标题/置顶 fallback 的持久身份排序和菜单边界用例。最终 UI713/0/1、presentation246/0/0、定向50/0/0、严格检查、普通demo build与独立public consumer执行通过，见[契约和收据](rust-agent-ui-sidebar-active-knownness-20261024.md)。本批全程复用SSD共享target；真实harness、桌面/IME/AX与TekesUI #53门槛仍开放，#22/#29不关闭。#23/#25只读复核没有新的独立实现缺陷，不重复已有交付。

### #15 / #18 / #27：当前 consumer CI 与 Native 无窗口诊断

`a2feb19` 的 push/PR CI 均完成：headless通过，UI workspace/严格Clippy/独立consumer执行通过，仅严格嵌套阅读失败；已下载两份原始artifact并核对PR synthetic merge与源closure，见[CI收据](rust-agent-ui-ci-a2feb19-20261024.md)。不以consumer通过放行#18/#28。

#27新增opt-in的隔离Kernel文件占用/锁观察采样，不改变归档行为或默认断言；Python101/0通过。冻结脚本源与当前headless程序后，有采样和无采样的两次独立Native0.9.3协议生命周期、真实Task来源与有界子历史均通过，见[诊断设计/结果](rust-agent-ui-native-lifecycle-diagnostics-20261024.md)。旧session-running未复现，原因和已修复均未证明；lsof锁字段为空，部分观察不证明锁可用性。未运行UI/桌面/IME/AX，也不升级三端同一窗口验收结论；Kernel/SDK/ClientKit只读，继续复用SSD共享target。#19真实producer仍未公开确认，#23/#25其余完整窗口/SDK子项继续保留。

### #22 / #29 / #27：真实三端同宿主 Sidebar 与共享验收工具

[最终收据](rust-agent-ui-real-multi-sidebar-results-20261024.md)记录 Native0.9.3、Pi1.3.0、DSH0.2.1-rc.2 在同一当前离屏 demo 的九个真实会话：逐端健康与库存/title/cwd，原分页 limit5→10/挂载6→9/原行ID稳定，三端查询完成、实际 display projection 无 unresolved，以及原导航 reveal 目标的 ContentHost 挂载。严格运行发现并修复分页抢焦点，直接消费 SDK 已有 preserving_focus 属性；合成预编辑与真实桌面验证分开。共享 Pi/Native/DSH/provider 工具接回旧协议脚本后，全三端原断言再通过；SSD空格路径 provider 修复与旧失败保留。最终 UI717/0/1、Python143、严格Clippy、普通demo和独立公开consumer通过，所有构建复用SSD共享target。

该交付不覆盖真实 Task-family（#23）、完整分类 producer（#29）、实际跨端同ID、真实故障/远端cursor、桌面/物理IME/AX或ProcessContainer。#23旧文档已注明 child-open 错误回归由 adbb7f3 完成。#12/#13 定向证据齐备但正文要求PR合并，仍保持open；#25 无新增可独立实施的状态缺陷。#53严格阅读门槛仍失败，不合并/打tag/发布；新head CI单独核对。

### #23 来源准备与 9b CI 回填

[真实家族来源预检](rust-agent-ui-real-task-family-source-results-20261024.md)已核对三端实际公开返回：Native父scope canonical Task与原始bounded页，Pi canonical Summary subagent与parent-bound history，DSH catalog与child presentation；源解析不授予pane/control authority。Python155通过，无Cargo/桌面。后续同一demo原intent/dispatch/挂载、受控迟到、A→B→A家族导航仍需完成；同Task row动态pointer公开producer未找到，#23不关闭。

[9b9ec1f CI收据](rust-agent-ui-ci-9b9ec1f-20261024.md)已下载PR/push两份原始artifact：headless/工作区/独立consumer通过，唯一失败仍是#53严格阅读。新提交的CI不得沿用该结果。

### #23：真实关联目录与原 child 分发完成本轮离屏闭环

[结果与限制](rust-agent-ui-real-task-family-results-20261024.md)记录三端各两个真实 parent/child 的目录控件→原 OpenChild/HostResult→原子历史绑定、A→B→A、正文节点、返回/关闭和父草稿保留。新增 FamilyCatalog 复用原 ChildCatalogState，使用不透明 owner key 防止同地址/同计数的新绑定复活旧意图；Native Task 保持非独立 Summary 行，bounded child 只读。关闭后旧结果错误已用红/绿回归修复，父切换时的合法缓存语义保留。UI734/0/1、Python161、严格Clippy、普通demo和六挂载独立consumer通过，全程使用SSD共享target。

同Task row真实动态pointer、完整窗口/chrome、物理IME/AX及#53仍开放，不能据本轮关闭#23或合并/发布。新增家族入口属于AgentKit高层控件，没有修改TekesUI基础组件。新head CI单独核对。


### #29：同帧分类与真实三端生命周期

[本轮结果](rust-agent-ui-real-sidebar-classification-results-20261024.md)补齐原 summary 字段的 Missing/Known/Unknown、同帧 publication 与搜索/宿主透传、同源归档事实、严格收据拒绝和原历史保留。真实 Native0.9.3/Pi1.3.0/DSH0.2.1-rc.2 的15个生命周期样本、当前Pi实例内新建scratch、原22个selected reads及六组家族导航通过；九会话Sidebar分页/焦点/reveal回归另通过。UI747/0/1、root340/0/0、Python170、严格Clippy/格式/边界与独立consumer执行通过；最后probe补充由普通demo构建和真实三端离屏运行另验。Pi scratch跨实例退休与Native无workspace归档事实单独记录，不制造main或archive authority。#29待新headCI及对应验收复核；#23动态Task pointer、桌面/IME/AX、#53仍开放，不合并发布。


### #22 / #24 / #27：真实非首端 Pi SideChat

[本轮结果](rust-agent-ui-real-multi-sidechat-results-20261024.md)用同一Native→Pi→DSH当前离屏demo的原控件/Host dispatcher，补齐Native/Pi实际Send、运行取消aborted、重连/重开/确认discard，Pi Create/后台Send/Close迟到跨端不抢选择或焦点，六个父Run草稿与历史隔离；DSH原入口保持disabled。11项定向、Python177、严格UI Clippy/格式/边界/矩阵、普通demo与独立consumer执行通过；源码522文件与最终程序SHA冻结。只读metadata和显式SDK编辑器Focus照原路径处理，不声称自动回焦/系统IME。完整#22/#24/#27、picker/附件、桌面/AX/ProcessContainer和#53仍开放，不能据本轮关闭整体Issue或合并发布。

#17的旧physical-planes文档已在d00f226标明BetweenInputs待办由42575ef交付取代，保留原阶段/收据身份，没有重新运行历史原生验收。#25另发现可补真实Summary子行status来源帧的验证：双读不能当同帧；已有一次性原构建边界见证的只读advisor建议，尚未实施或验证，不把它算作完成。

### #25 / #27：DSH 原参数标题与同帧完成来源

[本轮结果](rust-agent-ui-dsh-summary-source-results-20261024.md)修复真实DSH JSON字符串arguments丢失description标题的问题，按ClientKit解析一层且复用producer标题fallback，不改原Transcript/history或pointer/status权限。真实DSH0.2.1-rc.2的17条公开命令/7次合成请求、call seq16与result seq19配对、重连epoch2→5/原child pointer、公开子历史及原Summary挂载通过；raw inventory没有status，最新child turn/end seq26证明completed，未填造字段。root346/0、opt-in254/0、UI747/0/1、Python204、严格Clippy/格式/边界/矩阵、普通demo与独立公开consumer通过，均复用SSDtarget。#25/#27其余真实转换/草稿、完整布局/桌面/IME/AX及#53保持开放。

7e9383b的PR40090137271/push40090134330已结束：独立consumer/root341/feature249/UI747通过，唯一失败仍为#53严格阅读；原artifact/log及137/243文件闭包已核对。该终态不能替代DSH新提交CI。#16另继续实际宿主附件materialize共享路径；不把已有三端手工验收路径重复计为真实宿主回调完成。

### #16：实际宿主附件操作与离屏验收共用

[本轮结果](rust-agent-ui-attachment-host-operation-results-20261024.md)提取demo真实paths/read/upload/typed completion，保留原request身份、Kit门禁和原恢复状态，RFD构造仍在主UI线程。真实Native0.9.3/Pi1.3.0/DSH0.2.1-rc.2在同一当前程序逐端完成原附件policy/receipt/两类Send/草稿隔离/公开历史与新epoch重连；DSH不冒称通用文件回读。26项附件fixture、UI750/0/1、Python207、严格UI Clippy/格式/边界/矩阵、普通demo及复用的独立consumer执行通过。OS picker与原生HostResult消息循环、完整布局/焦点/桌面/物理IME/OS AX/ProcessContainer仍not_run，#16保持开放，不以共用逻辑代替桌面验收。全程SSD共享target，未改TekesUI基础组件。
