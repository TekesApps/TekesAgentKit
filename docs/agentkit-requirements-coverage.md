# AgentKit 需求与 Issue 覆盖索引

审计日期：2026-10-25。总 Epic [#14](https://github.com/TekesApps/TekesAgentKit/issues/14)。本索引是当前执行入口；[旧交付计划](rust-agent-ui-issue-plan.md)与[原Issue正文/状态](agentkit-issue-history-20261025.json)保留为历史，不能用其旧勾选代替当前集成状态。

[机器覆盖表](agentkit-issue-coverage-20261025.json)记录50条原始/最新契约要求、38个交付项及复用的#12/#13/#29（原43条/33票审计快照保留）。不为单个测试/每次验证建票。此表记录整理时的审计快照，实时执行状态以GitHub标签为准，后续交付同步更新本索引。

## 本阶段收口冻结（2026-10-25）

以 [阶段交付契约](agentkit-stage-close-20261025.md) 为当前执行顺序。代码冻结 `5b7c843`，ClientKit `c289bc9`、TekesUI `50ec49f`；后续仅做阻断修复，不追逐参照库新增行为。下文 `1d41ad4` 与其他旧程序结果保持历史身份。

#60–#66已同步到本索引与机器索引，全部为**后续待办**，不自动进入本阶段；完整Epic仍须满足其原始验收标准。#36新增跨格式恢复、#40悬停预览、#46未注册宿主适配保留WIP并冻结，不继续扩展。

当前 `5b7c843` root/UI格式、Clippy、测试、opt-in witness及独立公开API consumer均通过；UI CI在严格嵌套阅读检查失败（TekesUI #53），整体不能合入。Native queue另受Kernel #86真实失败阻塞。Native warm3仍仅证明有界子历史与passive Summary；新增原父Task审批拒绝失败与Pi primary-child transition来源窄见证已通过。Pi取消和Unknown原ticket自动对账已按独立新程序验收，均不升级为完整三端Family/Run或性能通过。详见[阶段结果](agentkit-stage-protocol-results-20261025.json)。#55/#56仍未完成。

## 边界与证据基线

AgentKit代码基线 `1d41ad4` / draft PR #10；远端main `f1daa0f`（v0.3.8 headless）。ClientKit提交 `c289bc9`、Tekes `8858ccd` 只读。ClientKit工作树的StandaloneComposer、提交生命周期、精确artifact引用、AgentKitRemote为未提交候选，语义差异已纳入新票，不能当成已发布接口。

只消费公开精确TekesUI `50ec49f`。业务组件/presentation归AgentKit；基础组件/IME/阅读/渲染归TekesUI；窗口/导航/发现/Keychain/权限归产品。此库无Swift，不改Kernel/SDK/外部WIP。

1d41ad4：本地UI790/0/1、定向default/feature各15、Clippy和独立consumer通过。push40143799874/PR40143802793只有严格TekesUI53失败，整体failure；原artifact/log、138输入、SDK8包243文件闭包与PR parent已核验。新Native Summary WIP未编译通过/未验收；未把它写入已验证结果。

历史归档的真实协议、离屏、组件、桌面、物理IME、OS AX、性能、发布分别解释。固定1026格矩阵仍绑定91129d7，不自动升级为当前HEAD。

## 当前看板摘要

实施票共41张（原33张，加旁支已建#60–#66及实际阻断缺陷#67），不含Epic和14张功能追踪。

| 状态 | 数量 | AgentKit Issue |
| --- | --- | --- |
| 已关闭 | 3 | #30–#32：已发布的历史里程碑 |
| 已验证待集成 | 10 | #12、#13、#29、#33、#34、#38、#41–#44 |
| 进行中 | 10 | #35–#37、#40、#45、#46、#55、#56、#58、#67 |
| 待办 | 8 | #59；#60–#66为后续待办 |
| 等待上游 | 9 | #39、#47–#54；每票只等待自己的相关上游交付 |
| 等待桌面 | 1 | #57；当前不启动桌面 |

#14–#28保持Epic/功能追踪；#19的来源交付等待上游，其余追踪票聚合进行中，不表示所有子项正在同时开发。#58的集成前置是代码票达到“已验证待集成”，不是要求先关闭它们；代码票在合入后才能关闭，避免循环依赖。

## 六种状态与关闭规则

待办＝范围确定尚未开始；进行中＝有实际开发/验收工作；已验证待集成＝适用验证通过但交付代码未合入；等待上游＝仅本子项等待公开能力；等待桌面＝需要当前禁止运行的真实桌面；已关闭＝满足本票标准且必要实现已进入主线。每张实施票唯一状态、负责人@qf0421。

大功能票#15–#28只追踪能力，#14只做总Epic。代码、真实协议、桌面IME、上游消费、集成/发布分别有子票；相关依赖只阻它自己的子项。父票关闭需其完整原要求与适用子票都满足，不能按PR一次合并或测试计数自动关闭。

## 已完成的历史交付

三个冻结的已发布里程碑回填原PR/tag/CI与完成时间，不重新验收旧程序。v0.1.0首次CI受账单限制未启动保留；v0.1.1及后续原提交CI成功已核实。

- [#30](https://github.com/TekesApps/TekesAgentKit/issues/30) [历史交付] 三端 SEP、session pointer 与无 UI Run 基线；原完成时间 `2026-10-19T14:25:16Z`，范围与限制见票。

- [#31](https://github.com/TekesApps/TekesAgentKit/issues/31) [历史交付] 无 UI Task、子代理、SideChat 与 Summary 状态；原完成时间 `2026-10-17T21:52:32Z`，范围与限制见票。

- [#32](https://github.com/TekesApps/TekesAgentKit/issues/32) [历史交付] Pi 子代理与 Native 有界子历史接入；原完成时间 `2026-10-17T22:44:27Z`，范围与限制见票。

原#29关闭记录按新增集成标准更正：**重新打开，已验证待集成**。f399356仍不在main；原三端证据与原关闭时间保留。#12/#13同样等待集成，不提前关闭。

## 需求 → Issue → 实现 → 验证

| 要求 | 功能追踪 | 实施票与状态 | 已有实现/实际验证 | 剩余与阻塞 |
| --- | --- | --- | --- | --- |
| R01 Rust-only；AgentKit业务/presentation、TekesUI基础组件、产品host三方所有权 | [#15](https://github.com/TekesApps/TekesAgentKit/issues/15), [#21](https://github.com/TekesApps/TekesAgentKit/issues/21) | [#30](https://github.com/TekesApps/TekesAgentKit/issues/30) 已关闭、[#33](https://github.com/TekesApps/TekesAgentKit/issues/33) 已验证待集成、[#40](https://github.com/TekesApps/TekesAgentKit/issues/40) 进行中 | 已发布基线无Swift；当前公开API/owner审计8335231与1d41ad4独立consumer | 不移植Swift；RunSurface/ComponentDemo仅测试宿主；新基础缺口提上游 |
| R02 Native/Pi/DSH三个SEP与Kernel0.9.x公开协议 | [#15](https://github.com/TekesApps/TekesAgentKit/issues/15), [#27](https://github.com/TekesApps/TekesAgentKit/issues/27) | [#30](https://github.com/TekesApps/TekesAgentKit/issues/30) 已关闭、[#32](https://github.com/TekesApps/TekesAgentKit/issues/32) 已关闭、[#55](https://github.com/TekesApps/TekesAgentKit/issues/55) 进行中 | PR1/2/5已合入；Kernel0.9.0→0.9.1旧证据与当前0.9.3分开 | 当前同交付源码三端协议逐项验收；不以RPC ack证明行为 |
| R03 本地直接读取与内存投影、远端Mirror/cache策略独立于harness | [#15](https://github.com/TekesApps/TekesAgentKit/issues/15), [#27](https://github.com/TekesApps/TekesAgentKit/issues/27) | [#30](https://github.com/TekesApps/TekesAgentKit/issues/30) 已关闭、[#33](https://github.com/TekesApps/TekesAgentKit/issues/33) 已验证待集成、[#56](https://github.com/TekesApps/TekesAgentKit/issues/56) 进行中 | 已发布local/remote/locator与能力文档；当时真实范围见原收据 | 远端TLS/大历史性能未覆盖不得称完整；不绕私有数据库 |
| R04 session pointer/locator、只读会话文件、历史恢复与重连 | [#17](https://github.com/TekesApps/TekesAgentKit/issues/17), [#23](https://github.com/TekesApps/TekesAgentKit/issues/23), [#27](https://github.com/TekesApps/TekesAgentKit/issues/27) | [#30](https://github.com/TekesApps/TekesAgentKit/issues/30) 已关闭、[#32](https://github.com/TekesApps/TekesAgentKit/issues/32) 已关闭、[#56](https://github.com/TekesApps/TekesAgentKit/issues/56) 进行中、[#13](https://github.com/TekesApps/TekesAgentKit/issues/13) 已验证待集成 | root三SEP基线已发布；后续原实例/epoch/lease绑定在PR10 | 按公开pointer与来源读取；不能以scope相同替换旧实例 |
| R05 无UI Run/Transcript/Composer/连接状态及队列数据回传 | [#16](https://github.com/TekesApps/TekesAgentKit/issues/16), [#17](https://github.com/TekesApps/TekesAgentKit/issues/17), [#21](https://github.com/TekesApps/TekesAgentKit/issues/21) | [#30](https://github.com/TekesApps/TekesAgentKit/issues/30) 已关闭、[#34](https://github.com/TekesApps/TekesAgentKit/issues/34) 已验证待集成、[#40](https://github.com/TekesApps/TekesAgentKit/issues/40) 进行中、[#55](https://github.com/TekesApps/TekesAgentKit/issues/55) 进行中 | PR2与v0.1.0已交付；PR7/8/9/11维护至v0.3.8 | 新UI层不因root发布自动完成 |
| R06 无UI Task、子代理、SideChat、Summary状态 | [#23](https://github.com/TekesApps/TekesAgentKit/issues/23), [#24](https://github.com/TekesApps/TekesAgentKit/issues/24), [#25](https://github.com/TekesApps/TekesAgentKit/issues/25) | [#31](https://github.com/TekesApps/TekesAgentKit/issues/31) 已关闭、[#43](https://github.com/TekesApps/TekesAgentKit/issues/43) 已验证待集成、[#44](https://github.com/TekesApps/TekesAgentKit/issues/44) 已验证待集成、[#45](https://github.com/TekesApps/TekesAgentKit/issues/45) 进行中 | PR4/v0.2.0 72测试与原三端集成/CI；原缺能力仍unsupported | 完整控件/真实当前协议/桌面分别未完 |
| R07 Pi独立子代理和Native有界子历史只读公开接口 | [#17](https://github.com/TekesApps/TekesAgentKit/issues/17), [#23](https://github.com/TekesApps/TekesAgentKit/issues/23) | [#32](https://github.com/TekesApps/TekesAgentKit/issues/32) 已关闭、[#56](https://github.com/TekesApps/TekesAgentKit/issues/56) 进行中、[#13](https://github.com/TekesApps/TekesAgentKit/issues/13) 已验证待集成 | PR5/v0.3.0与原Kernel0.9.1已合入发布 | 当前Native0.9.3分页/epoch/错误不能自动从旧tag继承 |
| R08 reported/live usage、completed-turn fork与worker恢复通知 | [#17](https://github.com/TekesApps/TekesAgentKit/issues/17), [#20](https://github.com/TekesApps/TekesAgentKit/issues/20), [#21](https://github.com/TekesApps/TekesAgentKit/issues/21) | [#30](https://github.com/TekesApps/TekesAgentKit/issues/30) 已关闭、[#39](https://github.com/TekesApps/TekesAgentKit/issues/39) 等待上游、[#45](https://github.com/TekesApps/TekesAgentKit/issues/45) 进行中、[#55](https://github.com/TekesApps/TekesAgentKit/issues/55) 进行中 | PR7/8/9/11已合并且v0.3.8 CI39268820690成功 | 新glyph/窄宽/协议case保留各自未验收范围 |
| R09 public仓库、MIT、README/接入与固定版本文档 | [#15](https://github.com/TekesApps/TekesAgentKit/issues/15), [#28](https://github.com/TekesApps/TekesAgentKit/issues/28) | [#30](https://github.com/TekesApps/TekesAgentKit/issues/30) 已关闭、[#33](https://github.com/TekesApps/TekesAgentKit/issues/33) 已验证待集成、[#59](https://github.com/TekesApps/TekesAgentKit/issues/59) 待办 | PR3/v0.1.1已合入，CI39070654656成功；v0.3.8已发布 | UI仍0.0.0/publish=false，不能冒充正式UI包 |
| R10 连接→Transcript→Composer→发送/取消→流式Rust demo复用presentation | [#15](https://github.com/TekesApps/TekesAgentKit/issues/15), [#21](https://github.com/TekesApps/TekesAgentKit/issues/21) | [#33](https://github.com/TekesApps/TekesAgentKit/issues/33) 已验证待集成、[#34](https://github.com/TekesApps/TekesAgentKit/issues/34) 已验证待集成、[#38](https://github.com/TekesApps/TekesAgentKit/issues/38) 已验证待集成、[#40](https://github.com/TekesApps/TekesAgentKit/issues/40) 进行中 | PR6已有实验prototype；当前RunPort标准组合在PR10 | 完成ClientKit View映射；测试宿主构造不是标准产品组件 |
| R11 Composer每目的地草稿/输入选择/附件/提交取消与unknown语义 | [#16](https://github.com/TekesApps/TekesAgentKit/issues/16) | [#34](https://github.com/TekesApps/TekesAgentKit/issues/34) 已验证待集成、[#35](https://github.com/TekesApps/TekesAgentKit/issues/35) 进行中、[#55](https://github.com/TekesApps/TekesAgentKit/issues/55) 进行中 | 原callback、attachment ticket和owner回归；1d41ad4 UI790/0/1只作为组件包验证 | 卸载后协调新差异与三端当前源码闭环未完成；物理IME另列 |
| R12 用户输入圆角气泡靠右/footnote靠右，机器输出/aside靠左；跟随SDK整体主题 | [#17](https://github.com/TekesApps/TekesAgentKit/issues/17), [#20](https://github.com/TekesApps/TekesAgentKit/issues/20), [#21](https://github.com/TekesApps/TekesAgentKit/issues/21) | [#38](https://github.com/TekesApps/TekesAgentKit/issues/38) 已验证待集成、[#39](https://github.com/TekesApps/TekesAgentKit/issues/39) 等待上游、[#40](https://github.com/TekesApps/TekesAgentKit/issues/40) 进行中、[#57](https://github.com/TekesApps/TekesAgentKit/issues/57) 等待桌面 | 现有业务列/原token/布局回归；不复制TekesUI主题 | Pi窄宽goal labelVisible=false保留失败；真实窗口尺度主题尚未通过 |
| R13 Transcript历史/stream/reasoning/tool与稳定source顺序 | [#17](https://github.com/TekesApps/TekesAgentKit/issues/17) | [#38](https://github.com/TekesApps/TekesAgentKit/issues/38) 已验证待集成、[#55](https://github.com/TekesApps/TekesAgentKit/issues/55) 进行中、[#56](https://github.com/TekesApps/TekesAgentKit/issues/56) 进行中 | 原内容/事件位置/工具组与流式回归；历史协议属于各自冻结程序 | 新协议源码需重新证明，不从测试计数推导全部Views |
| R14 富文本、代码、链接、图片历史与选择复制 | [#17](https://github.com/TekesApps/TekesAgentKit/issues/17) | [#47](https://github.com/TekesApps/TekesAgentKit/issues/47) 等待上游、[#36](https://github.com/TekesApps/TekesAgentKit/issues/36) 进行中 | 基础文本/工具已有；richContent/completeTranscript明确未完 | 文本/选择等待TekesUI25/54；位图图片等待TekesUI117；精确来源类型可独立实现 |
| R15 工具报告metadata与独立旁白/group fallback的合法公开来源 | [#19](https://github.com/TekesApps/TekesAgentKit/issues/19) | [#54](https://github.com/TekesApps/TekesAgentKit/issues/54) 等待上游 | 29c2f37完成来源政策审计与受控回归 | 真实公开producer/字段/更新语义未确认；不能从prose/didDo/result_meta猜造 |
| R16 历史分页/自动reveal/目标行可见性和同pane追加阅读位置 | [#18](https://github.com/TekesApps/TekesAgentKit/issues/18), [#22](https://github.com/TekesApps/TekesAgentKit/issues/22) | [#41](https://github.com/TekesApps/TekesAgentKit/issues/41) 已验证待集成、[#42](https://github.com/TekesApps/TekesAgentKit/issues/42) 已验证待集成、[#56](https://github.com/TekesApps/TekesAgentKit/issues/56) 进行中 | bd5e7f5原SDKwheel、可见行24→48、Y=-22保持；b0fa Native仅一Earlier至EOF | 多真实页shape/当前源码协议另证；非新Host或嵌套阅读 |
| R17 销毁后新ContentHost的持久阅读检查点 | [#18](https://github.com/TekesApps/TekesAgentKit/issues/18) | [#50](https://github.com/TekesApps/TekesAgentKit/issues/50) 等待上游 | 现有同pane阅读不构成检查点恢复 | 等待TekesUI42公开接口；禁止私有Anchor/SavedDocument替代 |
| R18 内层向上阅读通知父容器、追加不跳和不重复滚动 | [#18](https://github.com/TekesApps/TekesAgentKit/issues/18) | [#51](https://github.com/TekesApps/TekesAgentKit/issues/51) 等待上游 | 1d41ad4两份CI严格case失败；原raw accepted=false | 等待TekesUI53；不skip/弱化严格消费者断言 |
| R19 Indicators/rail/turn导航与完整Run组合 | [#21](https://github.com/TekesApps/TekesAgentKit/issues/21) | [#40](https://github.com/TekesApps/TekesAgentKit/issues/40) 进行中、[#48](https://github.com/TekesApps/TekesAgentKit/issues/48) 等待上游 | d0c82b2/5fc496a/67b20e6等已实现部分 | 构造/hover/reading不同范围；完整View层级与可视验收未完 |
| R20 Sidebar多端列表/活动/置顶/搜索/目录/分页/共享原目标 | [#22](https://github.com/TekesApps/TekesAgentKit/issues/22) | [#42](https://github.com/TekesApps/TekesAgentKit/issues/42) 已验证待集成、[#56](https://github.com/TekesApps/TekesAgentKit/issues/56) 进行中、[#29](https://github.com/TekesApps/TekesAgentKit/issues/29) 已验证待集成 | 现有多端、knownness、selected补读和原端路由，#29分类原证据保留 | 真实当前连接/回包闭环与桌面菜单另证 |
| R21 main/archived/ephemeral/subagent分类与成员关系 | [#22](https://github.com/TekesApps/TekesAgentKit/issues/22) | [#29](https://github.com/TekesApps/TekesAgentKit/issues/29) 已验证待集成 | f399356三端原公开producer+15生命周期样本；本次重开已验证待集成 | PR10未合入，不因旧分项关闭记录当已集成 |
| R22 非当前行上下文菜单及过滤/切端后原动作目标 | [#22](https://github.com/TekesApps/TekesAgentKit/issues/22) | [#52](https://github.com/TekesApps/TekesAgentKit/issues/52) 等待上游 | selected-row菜单已有，较窄候选未当作本项通过 | 等待TekesUI44事件/锚点；不替换成点击当前会话菜单 |
| R23 Task动态pointer、加载空错态、same-scope owner退休与父A→B→A | [#23](https://github.com/TekesApps/TekesAgentKit/issues/23) | [#43](https://github.com/TekesApps/TekesAgentKit/issues/43) 已验证待集成、[#12](https://github.com/TekesApps/TekesAgentKit/issues/12) 已验证待集成、[#56](https://github.com/TekesApps/TekesAgentKit/issues/56) 进行中 | 1d41ad4原red→green；最终default/feature各15；旧Run ready前后均拒绝 | 真实Task producer组合、窗口/chrome仍未验收 |
| R24 父来源权限继承/异步迟到与只读源实例身份 | [#23](https://github.com/TekesApps/TekesAgentKit/issues/23) | [#12](https://github.com/TekesApps/TekesAgentKit/issues/12) 已验证待集成、[#13](https://github.com/TekesApps/TekesAgentKit/issues/13) 已验证待集成、[#43](https://github.com/TekesApps/TekesAgentKit/issues/43) 已验证待集成 | f6cd531及后续1ef2791/公开consumer/source=None受控回归 | 都未合入PR10；已验证待集成不能关闭 |
| R25 SideChat临时分支create/send/reopen/close与能力门禁 | [#24](https://github.com/TekesApps/TekesAgentKit/issues/24) | [#44](https://github.com/TekesApps/TekesAgentKit/issues/44) 已验证待集成、[#56](https://github.com/TekesApps/TekesAgentKit/issues/56) 进行中 | Native/Pi原程序有协议/离屏分项，原DSH无discard时禁用 | 不把unsupported当实现；新公开能力再做适用接入；桌面独立 |
| R26 Summary sections/usage/metadata/host slots与原来源动作 | [#25](https://github.com/TekesApps/TekesAgentKit/issues/25) | [#45](https://github.com/TekesApps/TekesAgentKit/issues/45) 进行中、[#56](https://github.com/TekesApps/TekesAgentKit/issues/56) 进行中 | 原Pi/DSH、Native passive Task收据各有范围；不能混程序 | Native parent Task运行→失败当前WIP未编译通过；不是子代理圆点来源 |
| R27 最新ClientKit独立Composer同文档宿主、reparent、detach提交协调 | [#16](https://github.com/TekesApps/TekesAgentKit/issues/16), [#21](https://github.com/TekesApps/TekesAgentKit/issues/21) | [#35](https://github.com/TekesApps/TekesAgentKit/issues/35) 进行中、[#57](https://github.com/TekesApps/TekesAgentKit/issues/57) 等待桌面 | 当前CK未提交候选已只读冻结；Rust StandaloneComposerBinding/Mount、public consumer/demo候选已接入，未编译 | 新遗漏，复用Rustledger；原草稿状态不能证明此生命周期 |
| R28 精确artifact选择引用、revision/digest/UTF-16与stale/unavailable解析 | [#16](https://github.com/TekesApps/TekesAgentKit/issues/16), [#17](https://github.com/TekesApps/TekesAgentKit/issues/17), [#18](https://github.com/TekesApps/TekesAgentKit/issues/18) | [#36](https://github.com/TekesApps/TekesAgentKit/issues/36) 进行中 | typed artifactReference已接通原annotation/compile/history/reveal及consumer候选，尚未编译 | 新遗漏；不能相似文本重定向；数据契约可先做 |
| R29 WebUI/AgentKitRemote跨宿主URL/auth/stream/history/close语义 | [#15](https://github.com/TekesApps/TekesAgentKit/issues/15), [#26](https://github.com/TekesApps/TekesAgentKit/issues/26) | [#37](https://github.com/TekesApps/TekesAgentKit/issues/37) 进行中、[#55](https://github.com/TekesApps/TekesAgentKit/issues/55) 进行中、[#56](https://github.com/TekesApps/TekesAgentKit/issues/56) 进行中 | 已核对现有TekesWebUI HTTP/bridge；Rust JSONL DTO/尾帧/取消候选未编译，真实串联未验收 | 新遗漏；Swift adapter归ClientKit，Rust契约/服务归本库，不能把WIP当已发布pin |
| R30 紧凑Footnote glyph动作及完整label/权限/焦点 | [#20](https://github.com/TekesApps/TekesAgentKit/issues/20), [#21](https://github.com/TekesApps/TekesAgentKit/issues/21) | [#48](https://github.com/TekesApps/TekesAgentKit/issues/48) 等待上游、[#39](https://github.com/TekesApps/TekesAgentKit/issues/39) 等待上游 | 数据/CAS已有但glyph未接受 | 等待TekesUI47；无glyph不应阻塞可做的布局 |
| R31 feedback备注popover交互与原CAS/未知结果处理 | [#20](https://github.com/TekesApps/TekesAgentKit/issues/20) | [#49](https://github.com/TekesApps/TekesAgentKit/issues/49) 等待上游 | 既有CAS测试不等同实际Popover交互 | 等待TekesUI49，仅该消费项阻塞 |
| R32 RunPort/Sidebar/SidePanel同进程消费宿主与拆挂生命周期 | [#26](https://github.com/TekesApps/TekesAgentKit/issues/26) | [#46](https://github.com/TekesApps/TekesAgentKit/issues/46) 进行中、[#33](https://github.com/TekesApps/TekesAgentKit/issues/33) 已验证待集成 | ContentHost/第二consumer构造是基线 | 待做公开host adapter；不自造同进程ProcessContainer或把test host当产品 |
| R33 三个正式业务ProcessContainer跨进程输入/IME/scroll/AX/teardown | [#26](https://github.com/TekesApps/TekesAgentKit/issues/26) | [#53](https://github.com/TekesApps/TekesAgentKit/issues/53) 等待上游 | 现有public pin不具备完整跨进程验收 | 等待TekesUI48/64适用公开交接；和同进程adapter分开 |
| R34 当前交付源码三端Run原控件/数据回传/重连/队列附件闭环 | [#27](https://github.com/TekesApps/TekesAgentKit/issues/27) | [#55](https://github.com/TekesApps/TekesAgentKit/issues/55) 进行中 | 各原历史程序有证据，固定矩阵功能基线91129d7 | Native busy原Kernel86失败明确保留；不能用新源码复用旧程序通过 |
| R35 当前交付源码session/history/Task-family/Summary真实来源闭环 | [#27](https://github.com/TekesApps/TekesAgentKit/issues/27) | [#56](https://github.com/TekesApps/TekesAgentKit/issues/56) 进行中 | b0fa真实Native两页、后续同paneCPU与Pi/DSH原来源分别归档 | 同交付源码重新核验未完，当前Native Summary WIP有编译/validator待修 |
| R36 真实窗口/中文物理IME/焦点Undo/阅读布局/AX/performance | [#27](https://github.com/TekesApps/TekesAgentKit/issues/27) | [#57](https://github.com/TekesApps/TekesAgentKit/issues/57) 等待桌面 | 现有组件模拟Commit/CPU布局/离屏不算物理IME/OS AX | 等待桌面，用户明确先不跑；必要SDK只阻相应场景 |
| R37 固定宿主path/bundleID/稳定签名与单一截图入口 | [#15](https://github.com/TekesApps/TekesAgentKit/issues/15), [#27](https://github.com/TekesApps/TekesAgentKit/issues/27) | [#33](https://github.com/TekesApps/TekesAgentKit/issues/33) 已验证待集成、[#57](https://github.com/TekesApps/TekesAgentKit/issues/57) 等待桌面 | 固定工具链契约已写；历史发起截图审计/收据保留 | 真实签名安装/TCC未验收；不得每轮运行历史应用身份或触发新授权 |
| R38 CI独立consumer/review/严格门禁/合入与代码票关闭 | [#28](https://github.com/TekesApps/TekesAgentKit/issues/28) | [#58](https://github.com/TekesApps/TekesAgentKit/issues/58) 进行中 | 1d精确CI原artifact/log/138输入/SDK8包243文件及merge-parent已核验 | 整体仍因SDK53失败；无review记录；不能merge/tag或提前关代码票 |
| R39 UI公开精确版本/immutable阶段tag/文档与consumer交接 | [#28](https://github.com/TekesApps/TekesAgentKit/issues/28) | [#30](https://github.com/TekesApps/TekesAgentKit/issues/30) 已关闭、[#59](https://github.com/TekesApps/TekesAgentKit/issues/59) 待办 | root v0.3.8已发布；UI0.0.0未发布 | 当前UI需集成与明确发布范围；不可暗示完整Views完成 |
| R40 Issue层级、需求覆盖、分层证据、状态/负责人和时间 | [#14](https://github.com/TekesApps/TekesAgentKit/issues/14), [#27](https://github.com/TekesApps/TekesAgentKit/issues/27), [#28](https://github.com/TekesApps/TekesAgentKit/issues/28) | [#58](https://github.com/TekesApps/TekesAgentKit/issues/58) 进行中 | 本次保存原Issue正文/状态、PR/tag/CI清单与覆盖数据 | 审核无遗漏/重复、实际GitHub父子/状态/关闭核对后完成本次整理 |
| R41 复用SSD target、控制可再生缓存、WIP/固定快照/证据保留 | [#15](https://github.com/TekesApps/TekesAgentKit/issues/15), [#28](https://github.com/TekesApps/TekesAgentKit/issues/28) | [#33](https://github.com/TekesApps/TekesAgentKit/issues/33) 已验证待集成、[#58](https://github.com/TekesApps/TekesAgentKit/issues/58) 进行中 | target/ui target/fixtures已是SSD链接，构建用shared-target；原磁盘审计与dedup保留 | 共享target仍用于后续验证不整删；本次整理不新target/不编译/不清未知用途 |
| R42 三类组件/消费宿主/第二公开consumer共同验证，完整View不从smoke推导 | [#15](https://github.com/TekesApps/TekesAgentKit/issues/15), [#21](https://github.com/TekesApps/TekesAgentKit/issues/21), [#26](https://github.com/TekesApps/TekesAgentKit/issues/26), [#27](https://github.com/TekesApps/TekesAgentKit/issues/27) | [#33](https://github.com/TekesApps/TekesAgentKit/issues/33) 已验证待集成、[#40](https://github.com/TekesApps/TekesAgentKit/issues/40) 进行中、[#46](https://github.com/TekesApps/TekesAgentKit/issues/46) 进行中、[#55](https://github.com/TekesApps/TekesAgentKit/issues/55) 进行中、[#56](https://github.com/TekesApps/TekesAgentKit/issues/56) 进行中、[#57](https://github.com/TekesApps/TekesAgentKit/issues/57) 等待桌面 | 源码/内存SEP/协议/离屏/桌面/IME/AX/发布证据分层 | 全部原始要求保持；阶段合并不自动关闭大功能追踪或升级旧程序 |
| R43 Agent模型/设置选择、能力可见性与原宿主配置回调 | [#15](https://github.com/TekesApps/TekesAgentKit/issues/15), [#16](https://github.com/TekesApps/TekesAgentKit/issues/16), [#21](https://github.com/TekesApps/TekesAgentKit/issues/21) | [#33](https://github.com/TekesApps/TekesAgentKit/issues/33) 已验证待集成、[#34](https://github.com/TekesApps/TekesAgentKit/issues/34) 已验证待集成、[#35](https://github.com/TekesApps/TekesAgentKit/issues/35) 进行中 | 已有ComposerChoices和原controls配置门禁；产品场景参数/凭据保持host ownership | 独立宿主新生命周期另做；真实设置回传归PROTO-RUN，物理IME归DESKTOP |

| R44 [实施/SidePanel] 工具类 Tabs 注册、保留与关闭生命周期 | #14 | [#60](https://github.com/TekesApps/TekesAgentKit/issues/60) 待办 | 已建票；尚未实施或验收 | 后续范围；完整验收见原Issue，不作为本阶段自动前置 |
| R45 [实施/Files] 项目文件树、目录加载与原资源导航组件 | #14 | [#61](https://github.com/TekesApps/TekesAgentKit/issues/61) 待办 | 已建票；尚未实施或验收 | 后续范围；完整验收见原Issue，不作为本阶段自动前置 |
| R46 [实施/Changes] Git 变更列表、Diff 浏览与原版本动作组件 | #14 | [#62](https://github.com/TekesApps/TekesAgentKit/issues/62) 待办 | 已建票；尚未实施或验收 | 后续范围；完整验收见原Issue，不作为本阶段自动前置 |
| R47 [实施/Terminal] 交互终端面板、PTY 宿主适配与资源生命周期 | #14 | [#63](https://github.com/TekesApps/TekesAgentKit/issues/63) 待办 | 已建票；尚未实施或验收 | 后续范围；完整验收见原Issue，不作为本阶段自动前置 |
| R48 [实施/Artifact] 可复用文件预览、查看器扩展与精确上下文回传 | #14 | [#64](https://github.com/TekesApps/TekesAgentKit/issues/64) 待办 | 已建票；尚未实施或验收 | 后续范围；完整验收见原Issue，不作为本阶段自动前置 |
| R49 [实施/Browser] 浏览器面板契约、导航状态与 Composer 上下文桥接 | #14 | [#65](https://github.com/TekesApps/TekesAgentKit/issues/65) 待办 | 已建票；尚未实施或验收 | 后续范围；完整验收见原Issue，不作为本阶段自动前置 |
| R50 [实施/Composer] Command Palette：+ / @、命令目录与作用域解析 | #14 | [#66](https://github.com/TekesApps/TekesAgentKit/issues/66) 待办 | 已建票；尚未实施或验收 | 后续范围；完整验收见原Issue，不作为本阶段自动前置 |

## 实施队列与负责人

所有AgentKit实施票负责人均为@qf0421；主会话协调/审查/集成，使用新子会话实施，不联系已结束旧子会话。上游issue仍由上游维护者负责，本库负责消费/跟进。
| 实施票 | 主追踪 | 层 | 状态 | 交付物 |
| --- | --- | --- | --- | --- |
| [#30](https://github.com/TekesApps/TekesAgentKit/issues/30) [历史交付] 三端 SEP、session pointer 与无 UI Run 基线 | [#14](https://github.com/TekesApps/TekesAgentKit/issues/14) | 历史交付 | 已关闭 | 已发布Rust-only库与公开接入文档；历史提交/CI/tag对应记录。 |
| [#31](https://github.com/TekesApps/TekesAgentKit/issues/31) [历史交付] 无 UI Task、子代理、SideChat 与 Summary 状态 | [#14](https://github.com/TekesApps/TekesAgentKit/issues/14) | 历史交付 | 已关闭 | presentation状态与三SEP规范化；已合并发布的历史证据。 |
| [#32](https://github.com/TekesApps/TekesAgentKit/issues/32) [历史交付] Pi 子代理与 Native 有界子历史接入 | [#14](https://github.com/TekesApps/TekesAgentKit/issues/14) | 历史交付 | 已关闭 | 两个公开SEP能力、能力矩阵和原版本集成证据。 |
| [#33](https://github.com/TekesApps/TekesAgentKit/issues/33) [实施/API] 公共 Rust 组件契约、所有权和独立 consumer | [#15](https://github.com/TekesApps/TekesAgentKit/issues/15) | 实现 | 已验证待集成 | 接入契约、公开API/边界检查、无内部demo依赖的独立consumer与文档。 |
| [#34](https://github.com/TekesApps/TekesAgentKit/issues/34) [实施/Composer] 草稿、控件、附件输入与发送取消组件 | [#16](https://github.com/TekesApps/TekesAgentKit/issues/16) | 实现 | 已验证待集成 | 可复用Composer与原RunBinding回调，跨目的地/迟到结果隔离和公开consumer。 |
| [#35](https://github.com/TekesApps/TekesAgentKit/issues/35) [实施/Composer] 独立宿主生命周期与卸载后提交协调 | [#16](https://github.com/TekesApps/TekesAgentKit/issues/16) | 实现 | 进行中 | Rust公开宿主控制契约及原提交身份/未解决状态重挂载协调；差异与公开交接记录。 |
| [#36](https://github.com/TekesApps/TekesAgentKit/issues/36) [实施/Context] 精确批注来源引用与失效解析 | [#16](https://github.com/TekesApps/TekesAgentKit/issues/16) | 实现 | 进行中 | Rust类型/编译与解析结果契约、宿主reveal回调；与现有annotation/source_locator衔接。 |
| [#37](https://github.com/TekesApps/TekesAgentKit/issues/37) [实施/WebUI] AgentKitRemote 跨宿主协议与接入契约 | [#15](https://github.com/TekesApps/TekesAgentKit/issues/15) | 实现 | 进行中 | 公开协议/能力与错误语义、跨宿主consumer契约及精确版本交接；Swift adapter仍归ClientKit，discovery/Keychain归产品。 |
| [#38](https://github.com/TekesApps/TekesAgentKit/issues/38) [实施/Transcript] 原始历史、流式、思考与工具内容组件 | [#17](https://github.com/TekesApps/TekesAgentKit/issues/17) | 实现 | 已验证待集成 | 共享内容树与原presentation映射、只读/流式门禁和公开组件回调。 |
| [#39](https://github.com/TekesApps/TekesAgentKit/issues/39) [实施/Footnote] 原动作与元信息的对齐和窄宽布局 | [#20](https://github.com/TekesApps/TekesAgentKit/issues/20) | 实现 | 等待上游 | 原业务Footnote及反馈数据回调的布局修复与可审查组件结果。 |
| [#40](https://github.com/TekesApps/TekesAgentKit/issues/40) [实施/RunPort] 标准 Run 组合与 ClientKit View 映射 | [#21](https://github.com/TekesApps/TekesAgentKit/issues/21) | 实现 | 进行中 | 复用presentation的公开Run组合组件、输入/回调和View差异表。 |
| [#41](https://github.com/TekesApps/TekesAgentKit/issues/41) [实施/Reading] 同 pane 分页、自动 reveal 与重连阅读位置 | [#18](https://github.com/TekesApps/TekesAgentKit/issues/18) | 实现 | 已验证待集成 | 公开阅读输入/意图、原行/文档身份与同pane组件/consumer证明。 |
| [#42](https://github.com/TekesApps/TekesAgentKit/issues/42) [实施/Sidebar] 多端会话导航、搜索、分页及原目标动作 | [#22](https://github.com/TekesApps/TekesAgentKit/issues/22) | 实现 | 已验证待集成 | 共享Sidebar与demo宿主路由、原address/owner/请求意图与consumer。 |
| [#43](https://github.com/TekesApps/TekesAgentKit/issues/43) [实施/SidePanel] Task/child tabs、内容 owner 与父导航生命周期 | [#23](https://github.com/TekesApps/TekesAgentKit/issues/23) | 实现 | 已验证待集成 | 复用SidePanel/TaskPaneBinding/ChildPaneBinding的公开业务组件与原事件宿主接入。 |
| [#44](https://github.com/TekesApps/TekesAgentKit/issues/44) [实施/SideChat] 临时分支的原 Run、重开与能力门禁 | [#24](https://github.com/TekesApps/TekesAgentKit/issues/24) | 实现 | 已验证待集成 | 原SideChatBinding/BoundSideChatRun与SidePanel控件回调、能力矩阵和错误/unknown门禁。 |
| [#45](https://github.com/TekesApps/TekesAgentKit/issues/45) [实施/Summary] sections、usage、metadata 与原来源动作组件 | [#25](https://github.com/TekesApps/TekesAgentKit/issues/25) | 实现 | 进行中 | 共享SummaryView、原输入/动作/定制slot与业务布局契约。 |
| [#46](https://github.com/TekesApps/TekesAgentKit/issues/46) [实施/Host] 三个业务组件的同进程宿主适配与生命周期 | [#26](https://github.com/TekesApps/TekesAgentKit/issues/26) | 实现 | 进行中 | Rust侧公开宿主adapter/consumer与生命周期文档；窗口/产品导航/权限仍属产品。 |
| [#47](https://github.com/TekesApps/TekesAgentKit/issues/47) [上游消费/Transcript] 富文本、代码、链接、图片与选择复制 | [#17](https://github.com/TekesApps/TekesAgentKit/issues/17) | 上游消费 | 等待上游 | 公开精确pin升级、共享渲染适配与独立consumer；不实现替代renderer。 |
| [#48](https://github.com/TekesApps/TekesAgentKit/issues/48) [上游消费/Actions] 原 SDK glyph 与紧凑业务动作接入 | [#20](https://github.com/TekesApps/TekesAgentKit/issues/20) | 上游消费 | 等待上游 | 精确SDK升级、业务图标服务适配和窄宽组件consumer。 |
| [#49](https://github.com/TekesApps/TekesAgentKit/issues/49) [上游消费/Feedback] 评价备注 popover 与原 CAS 提交 | [#20](https://github.com/TekesApps/TekesAgentKit/issues/20) | 上游消费 | 等待上游 | 业务备注交互控件与原CAS回调；公开SDK精确pin和consumer。 |
| [#50](https://github.com/TekesApps/TekesAgentKit/issues/50) [上游消费/Reading] 跨新建 ContentHost 的持久阅读恢复 | [#18](https://github.com/TekesApps/TekesAgentKit/issues/18) | 上游消费 | 等待上游 | 可持久化公开checkpoint类型和AgentKit存取回调/consumer。 |
| [#51](https://github.com/TekesApps/TekesAgentKit/issues/51) [上游消费/Reading] 嵌套详情阅读意图与父跟随 | [#18](https://github.com/TekesApps/TekesAgentKit/issues/18) | 上游消费 | 等待上游 | 公开精确SDKpin及原严格nested-detail consumer通过。 |
| [#52](https://github.com/TekesApps/TekesAgentKit/issues/52) [上游消费/Sidebar] 非当前会话行菜单与原目标保持 | [#22](https://github.com/TekesApps/TekesAgentKit/issues/22) | 上游消费 | 等待上游 | 公开SDK事件/锚点消费、原pin/share/rename/archive/copy宿主回调。 |
| [#53](https://github.com/TekesApps/TekesAgentKit/issues/53) [上游消费/ProcessContainer] 三业务容器跨进程接入 | [#26](https://github.com/TekesApps/TekesAgentKit/issues/26) | 上游消费 | 等待上游 | 精确公开SDK、业务容器adapter及独立跨进程consumer；不改SDK。 |
| [#54](https://github.com/TekesApps/TekesAgentKit/issues/54) [实施/Sources] 公开报告与独立旁白来源消费及合法降级 | [#19](https://github.com/TekesApps/TekesAgentKit/issues/19) | 真实协议 | 等待上游 | 共享source adapter与能力契约、原source position/revision和真实producer证据。 |
| [#55](https://github.com/TekesApps/TekesAgentKit/issues/55) [验收/协议] 当前交付源码的三端 Run 输入输出与恢复 | [#27](https://github.com/TekesApps/TekesAgentKit/issues/27) | 真实协议 | 进行中 | 程序/源码/锁/版本/原收据闭环与来源矩阵，独立公开consumer；产品修复回到对应实现票。 包含适用的非桌面性能profile/数据集/机器版本、延迟与内存基线；先在票中冻结有限交付清单和量化门槛。 |
| [#56](https://github.com/TekesApps/TekesAgentKit/issues/56) [验收/协议] 当前交付源码的 session、历史与家族面板闭环 | [#27](https://github.com/TekesApps/TekesAgentKit/issues/27) | 真实协议 | 进行中 | 各真实端原控件/原binding和公开数据的可追溯协议/离屏证据，保留typed source receipts和所有失败尝试。 包含适用的非桌面性能profile/数据集/机器版本、延迟与内存基线；先在票中冻结有限交付清单和量化门槛。 |
| [#57](https://github.com/TekesApps/TekesAgentKit/issues/57) [验收/桌面] 固定宿主的布局、阅读、中文 IME 与 AX | [#27](https://github.com/TekesApps/TekesAgentKit/issues/27) | 桌面IME | 等待桌面 | 宿主身份/签名/权限收据和真实窗口分项证据；保留历史程序只归档及失败记录。 原生性能profile、同机同版本baseline、p50/p95响应/帧时间、jank和CPU/峰值内存记录。 |
| [#58](https://github.com/TekesApps/TekesAgentKit/issues/58) [交付/集成] PR #10 的完整范围审查、严格 CI 与合入 | [#28](https://github.com/TekesApps/TekesAgentKit/issues/28) | 发布 | 进行中 | 需求覆盖表、差异审查、精确head终态CI/原artifact/源码SDK闭包、合并及实施票状态核对。 |
| [#59](https://github.com/TekesApps/TekesAgentKit/issues/59) [交付/发布] 精确依赖的 UI 版本与阶段 tag | [#28](https://github.com/TekesApps/TekesAgentKit/issues/28) | 发布 | 待办 | 发布pin/版本、MIT/readme接入文档、immutable annotated tag、release/交接与剩余范围。 |
| [#12](https://github.com/TekesApps/TekesAgentKit/issues/12) 复用已有修复 | [#23](https://github.com/TekesApps/TekesAgentKit/issues/23) | 实现/原来源 | 已验证待集成 | f6cd531及后续原来源门禁证据，PR10未合入 |
| [#13](https://github.com/TekesApps/TekesAgentKit/issues/13) 复用已有修复 | [#23](https://github.com/TekesApps/TekesAgentKit/issues/23) | 实现/原来源 | 已验证待集成 | 原只读instance identity及1ef2791门禁证据，PR10未合入 |
| [#29](https://github.com/TekesApps/TekesAgentKit/issues/29) 复用已有修复 | [#22](https://github.com/TekesApps/TekesAgentKit/issues/22) | 实现/原来源 | 已验证待集成 | f399356原分类协议/组件证据；本次重开，PR10未合入 |

| [#60](https://github.com/TekesApps/TekesAgentKit/issues/60) [实施/SidePanel] 工具类 Tabs 注册、保留与关闭生命周期 | #14 | 后续实现 | 待办 | 完整范围/交付物/依赖/验收同步于机器索引；本阶段不展开。 |
| [#61](https://github.com/TekesApps/TekesAgentKit/issues/61) [实施/Files] 项目文件树、目录加载与原资源导航组件 | #14 | 后续实现 | 待办 | 完整范围/交付物/依赖/验收同步于机器索引；本阶段不展开。 |
| [#62](https://github.com/TekesApps/TekesAgentKit/issues/62) [实施/Changes] Git 变更列表、Diff 浏览与原版本动作组件 | #14 | 后续实现 | 待办 | 完整范围/交付物/依赖/验收同步于机器索引；本阶段不展开。 |
| [#63](https://github.com/TekesApps/TekesAgentKit/issues/63) [实施/Terminal] 交互终端面板、PTY 宿主适配与资源生命周期 | #14 | 后续实现 | 待办 | 完整范围/交付物/依赖/验收同步于机器索引；本阶段不展开。 |
| [#64](https://github.com/TekesApps/TekesAgentKit/issues/64) [实施/Artifact] 可复用文件预览、查看器扩展与精确上下文回传 | #14 | 后续实现 | 待办 | 完整范围/交付物/依赖/验收同步于机器索引；本阶段不展开。 |
| [#65](https://github.com/TekesApps/TekesAgentKit/issues/65) [实施/Browser] 浏览器面板契约、导航状态与 Composer 上下文桥接 | #14 | 后续实现 | 待办 | 完整范围/交付物/依赖/验收同步于机器索引；本阶段不展开。 |
| [#66](https://github.com/TekesApps/TekesAgentKit/issues/66) [实施/Composer] Command Palette：+ / @、命令目录与作用域解析 | #14 | 后续实现 | 待办 | 完整范围/交付物/依赖/验收同步于机器索引；本阶段不展开。 |

## 整理后的执行顺序

1. 冻结5b范围、运行时与有限#55/#56矩阵，保留所有历史证据与WIP。
2. 复用有效证据，只补受影响/缺失真实协议；主会话串行Cargo，冻结输入且复用SSD target。
3. #58审查已验证待集成项目；只有真实依赖隔离且保留全部适用门禁才拆PR。
4. 精确head review、CI、独立consumer通过后合入，核对main CI，再由#59发布标明范围的版本。
5. #57继续等待桌面；#60–#66及未注册扩展WIP留到后续。上游失败仅阻相关范围，不能降验收标准。

## WIP 与磁盘

初始Issue整理阶段（2c8b5a1）只改Issue/文档/治理，未启动新构建或真实验证；优先级切换前已启动的Cargo build退出失败，其日志保留。五个Native Summary候选文件已按SHA冻结到SSD审计目录；代码未混入本次文档提交。target、ui/target、fixtures均为既有SSD链接，所有Cargo继续用shared-target；未新建target、未清理用途不明缓存或改其他任务。
历史日志/收据/截图/验收可执行文件和固定SDK快照完整保留。此前去重的实际释放按原磁盘收据解释，本次Issue整理新增删除0字节。

## 整理后实施进度（2026-10-25）

#35独立Composer和#36精确引用已由新的独立子会话交付候选，主会话已接入公开consumer/demo测试源；#37沿用实际TekesWebUI桥接，公开JSONL DTO和尾帧/取消候选也已交付。三项候选已提交9c6b1be，仍未编译/协议验收，不是已验证待集成。源码SHA与候选保存在SSD `issues35-36-37-candidate-inputs`，原Cargo批次输入另存 `ui-workspace-loader-frozen-inputs`；新检查等待原批次结束后串行执行。

#45真实来源暴露doing→pending，来源适配修复的presentation 290项及独立Python verifier 11项通过；真实run1的Summary仍是负向，正式Task producer/父工具failure仍未验收，父spawn成功不与child error互换。新编译/真实profile重跑未完成。

#39左右对齐与内容宽度已有原证据；剩余窄列单行目标label+全部动作可见依赖#48，状态等待上游。真实Pi labelVisible=false保留，不能换行/隐藏/伪glyph通过。最新TekesUI公开main 7c13ec0相对固定pin的完整491文件Git差异未改243个SDK闭包输入，没有公开的嵌套阅读修复，固定pin不变。

当前实施票：3已关闭、10已验证待集成、9进行中、1待办、9等待上游、1等待桌面；原审计快照及历史证据保留。桌面/IME/AX继续不跑，未新建target或删除缓存。

#40追加主审：现有TurnRail就是indicatorrail的业务实现；新增重复投影/几何候选已原字节归档而未注册。剩余hover preview沿原组件与SDK公开overlay能力补齐，不新建第二套轨道。#46已开始同进程mount适配，模块尚未注册/编译。Native Summary候选提交e363863，正式范围仍待验收。


## 当前阶段新增阻断修复

[#67](https://github.com/TekesApps/TekesAgentKit/issues/67) 归属#27/#56：DSH合法cached列表提示与live有序投影隔离。它复用R02/R04/R20/R35，不新增功能要求或测试票；原5b失败、固定公开契约与补丁证据见[边界记录](agentkit-dsh-cached-authority-20261026.md)。只有该阻断修复进入本阶段；#60–#66仍为后续。
