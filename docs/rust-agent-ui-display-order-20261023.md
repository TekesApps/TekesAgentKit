# 原始位置与平面展示顺序（2026-10-23）

当前 ClientKit 的 SessionEventProjector.framePresentationOrder 将 committed assistant 的 presentation order 设为其 journal seq 与同 step 首个 call seq 的较小值；transient 使用当前 durable tail + 1。TranscriptMetalActivity 的平面 timeline 在相同 sink order 下先正文/说明、后工具，工具 tie 保留 chronologicalCallIDs。Tekes 宿主组合 ClientKit 的展示配置，不另排业务行。本阶段仅修改 AgentKit，消费原精确公开 TekesUI。

新增公开 `TranscriptSection::display_order(scope, sources) -> TranscriptDisplayOrder`。它复用原 `event_order()` 的 family/position/collision 校验，之后按原归一化 presentation provenance 排列展示条目；`TranscriptEventOrder`、`TranscriptSourcePosition`、原始 journal seq、native entry ordinal、history cursor 和业务 key 均不修改。可用来源当前由 `snapshot.thinking_sources` 这一已有归一化目录供应，它记录的是共享展示来源，不是 UI 行号或权限版本。

- 已知 assistant 用原 frame presentation anchor，已知 call 用其真实 call anchor；result 保留自己的 source position，不能借 call 的位置冒充结果到达顺序。
- 同 sink order 的消息先于工具；同记录工具保留原 content/call 顺序，不按工具 ID 字母排序。同位置消息使用原 ID 稳定 tie。
- Journal/Pi preview 可用明确的 normalized anchor 进入展示顺序；它仍没有 durable source position，也不会取得 NativeEntry/Submission 或阅读权限。
- 缺少目录的旧 snapshot 保持原 source order/unplaced。来源 scope 不符、负展示序号、源冲突和混合 family 拒绝。无任何已知 position/anchor 的记录继续显式 unplaced，不用时间戳、正文、phase 或列表下标猜排。

TranscriptContent 的原行 planner 现消费该投影，RunPort/ReadOnlyTranscript 和已发布 turn targets 沿用同一 planner；不新增缓存或状态机。原组件身份、tool run 成员控件、prepared plan、reading token 及 editor owner 保持既有边界。第二 public consumer 的 display_timeline 只调用共享 API，宿主不写另一套排序。

本阶段是消息容器/独立工具的平面顺序接线，不代表完整 ClientKit timeline：独立 reasoning/narration kind、narration carrier/fallback handoff、拆分消息内 reasoning/narration/tool planes、富文本与进程容器仍开放。正式 ProcessContainer 同/跨进程仍等待 TekesUI #48，嵌套阅读严格门禁 #53 不变，不以同进程 ContentHost 验收替代。

## 验证与新的原生输入观察

根 workspace 281 项、完整 UI workspace 589 项通过；UI 仅一项原有 #49 忽略。根/UI strict all-target Clippy、格式、边界与固定 SDK 输入检查通过。完整回归后只有测试断言的临时分配 lint 修正，最终 display-order 14 项再次通过，实现未改变。

新测试用真实 public Journal 数据经过 MemoryProjectionStore/PresentationStore：工具先落 journal、assistant 后提交，仍按同 step 首 call anchor 展示；调用 z→a 保持来源顺序，9999 transient ordinal 不作展示位置，snapshot 原值不变。Pi 历史前插后规范 key 不变，预览虽然已定位仍无 entry 身份。第二 public consumer 和 RunPort 复用该 API，工具分组、原行/editor、焦点和中文 preedit 保持。

最初完整回归中三项只读测试将父快照挂到虚构 child scope，被新的严格来源检查拒绝。已使这些纯渲染 fixture 使用原快照真实 scope，37 项只读/阅读用例通过；没有清空来源或重新贴 child 标签。真实 child/bounded source 原本分别在 child.rs/bounded_history.rs 用真实 child scope 调用 TranscriptSnapshot::read，本阶段未改这条链。纯渲染测试不是 bounded-child SEP 证据。

本阶段程序 SHA-256 `f4dbd88fceb79702ddc71df81b7219067c8dea3833c5357e72b483153e3aad2d` 的原生窗口阅读通过：本地 fixture 分页 20→44、前插/向上阅读、焦点与 preedit、A/B 草稿及重连恢复。

**系统拼音不记为稳定通过。** 本程序首次运行在前台身份正确、输入源核验成功时，首键 n 得到 Commit("n") 而非 Preedit，后续才进入 i hao marked 状态，最终 n i好，严格“你好”检查失败。旧冻结程序 6a3f043e 对照通过；同一 f4dbd88f 程序未改代码/参数/事件节奏的复跑也通过，包含发送/导航组字屏障和单次提交。保留首次失败，尚不能归因当前修改、SDK、AppKit 或注入来源；也没有加定时/丢键/自动重试补偿。该不同于 Unicode typeText 的事件链已补充 [TekesUI #41](https://github.com/TekesApps/TekesUI/issues/41#issuecomment-6351744540)，待独立原生控件对照。无物理键盘、候选窗或 VoiceOver 验收。

已核对 06d86f8 的终态 CI：两项基础检查通过、两项 macOS UI 失败，失败 job 仍为 unchanged strict bounded_detail_reading（#53）。不降低门禁，不合并或打 tag；新提交状态以 Checks 为准。

复用共享 target，关闭 incremental；新建 target 0，验收程序硬链接保留，删除/释放 0 字节。源码/WIP、固定依赖、原程序和失败/成功日志均保留。精确输入与收据见[机器记录](rust-agent-ui-display-order-20261023.json)。
