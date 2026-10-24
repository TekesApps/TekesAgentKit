# Issue #25 Summary 对照审计（2026-10-24）

本记录对照当前 TekesClientKit Summary provider/view、Tekes 宿主适配和 AgentKit Rust presentation/UI 组件。审计只覆盖可公开消费的 Summary facts、导航权限和状态隔离，不把 UI 像素或完整 ClientKit parity 当成本次目标。

## 已对照契约

| 范围 | ClientKit 当前行为 | AgentKit Rust 当前行为与结论 |
| --- | --- | --- |
| Tasks | `SummaryProviders.tasks` 只在 mirror 属于当前 thread 时显示；只有 `taskHasIndependentRun(sessionID, threadID)` 成立才回调 `openTask`。 | `SummaryFacts::sections` 只为独立 RunScope 创建 `OpenTask`；root todo 和没有独立会话 pointer 的 task 保持被动。符合边界。 |
| Subagents | `SummarySubagentPresentation` 从 producer 参数取 `brief` / `description`，从结果中提取第一个 UUID，再要求其精确属于已知 child session ID（或匹配一个已知 endpoint-qualified ID 后缀）；未知 pointer 不可导航。状态只映射 running/in-progress/working 与 failed/interrupted。 | 标题与状态规则相同。此前 `SummaryFacts::read` 会把工具结果任意文本中的 UUID 直接变成 `OpenSubagent`，即使 session 不在当前 endpoint inventory。已修为候选与授权分离：`read` 不授权导航；`read_with_store` 只接受同 endpoint 已知 session 的精确 ID，或唯一的 `:<candidate>` 后缀；父 session、未知 ID 和歧义后缀均不生成 pointer。若 parent 自身是 `dsh:session:<uuid>` 形式，也会按后缀识别并拒绝该 UUID，避免改投另一个带相同 UUID 的 session。状态仍只从该 child session 的公开 retained summary/activity 读取。 |
| Sources | 从显式 artifact `source` 去空白、去重、排序；没有 source 就不创建该 section。 | 同样只读取显式 `SummaryArtifact.source`，去空白、去重、排序；不从路径或 tool result 推造来源。符合边界。 |
| Artifacts | mirror newest-first；同一路径只显示最新项；行调用宿主 `changeCounts`、`openArtifact`。 | 显式 Artifact facts newest-first、按 ID 首条去重，提供 typed `OpenArtifact`。Git diff 和导航由 host input/action 承担；缺省 change-count 不造值。顺序是输入契约，调用方需继续提供 newest-first。 |
| Usage | 读取 mirror 提供的 aggregate；总量与 input/output/cache/hit-rate/cost 分行。分母为 uncached + cached + cache-creation input；零分母显示占位；不完整或缺失费用显示 Unavailable。 | 从公开 `tokenUsage` projection 读取各 token 计数。缺失 projection 时整个 usage section 不出现；缺失必需计数是 projection 错误，不填 0。成本保持 `None` 且 incomplete，除非未来有完整权威字段；不根据 token 数估价。hit-rate 零分母为 `None`。契约一致，缺省状态通过 localized presenter 显示。 |
| Metadata / absence | provider 不从缺失数据合成行值；unknown session/action 保持不可操作。 | `SummaryRow.value = null` 表示该行没有 value，未知成本/hit-rate 也序列化为 null；UI trailing formatter 不从状态补值。Subagent 结果出现 UUID 不等于 session 存在；本次修复后只有 inventory membership 才能授权 pointer。 |
| Host sections / callbacks | `SummaryContext` 注入 task/subagent/artifact callbacks、workspace `changeCounts` 与 `applicationSections`；application providers 和默认 registry 顺序组合。custom view 由 section/item identity 保留。 | `SummaryContextInput` 接收显式 host sections/custom rows；typed intents 让宿主执行导航；`SummaryCustomRowSpec` 和 token 将 owner/revision/disclosure epoch/content key 绑定原 host Content。原有 controls 通过 SDK lifecycle 安装、更新和退休，不复制 SDK tree validator。组件层没有要求 TekesUI 新增基础组件。组件 API 接收的显式 host `SummaryFacts` / sections 仍以宿主遵守事实来源、scope 和 callback 权限契约为边界；组件可做 owner/scope 结构检查，但不能验证宿主背后的业务 authority。 |
| Async/scope fencing | mirror/workspace query task 按 thread/workspace identity 取消和重建；view detach 时取消；观察回调通过 `observationGeneration` 丢弃过期通知；mirror rows 仅在 loaded thread 等于当前 thread 时可见。 | `SummaryState` 的 token 同时绑定 exact `RunScope` 与 generation，过期 token 忽略；提交还要求 facts scope 精确相等且有效。组件 publication 使用 monotonic revision 与 Run/Workspace owner；custom row token 会在 owner、revision、折叠 epoch 或 content key 变更后失效。已有隔离不变量覆盖本 issue，不需另造状态机。 |

## 本次修复

工具结果里的 UUID 只表示候选值。`SummaryFacts::read` 没有 session inventory 权限，因此其 subagent row 现在始终是被动行。Run presentation 走 `read_with_store`：先要求 parent scope 与 store endpoint 一致，再以同一 store 中的已知 session 解析候选。若候选等于 parent ID 或 parent ID 以 `:<candidate>` 结尾，立即拒绝，防止 namespace 下相同 UUID 被重定向到另一条 session。否则精确 ID 优先；只有唯一 `:<uuid>` 后缀匹配才授权，多个匹配时保持 unknown。status lookup 只在 pointer 已被授权后进行。

这让直接调用 `SummaryFacts::read` 的消费者必须使用 `read_with_store`（或显式 host-projected facts）才能提供可导航 subagent pointer。测试夹具已调整为体现“无 inventory = 被动、有同端已知 child = 可导航”。没有加入 child-parent 关系强校验：ClientKit 的契约是 known-session membership，而不是另一套关系推断。

## 源码证据

本审计检查的主要来源及 SHA-256：

- TekesClientKit `Sources/Composer/Agent/SummaryProviders.swift` — `1f2eb4048c30bfde58edd72bd2e50b8e22d211272c609c9d2108c53b6bddf663`
- TekesClientKit `Sources/Composer/Agent/SummaryPanelView.swift` — `72049a4d18530e799eeea3556eebd191c51383ad3c0d31ce1308d5daaf7c8246`
- TekesClientKit `Sources/Composer/Agent/ClientAgentSummaryView.swift` — `5632d920bd21d44d6b68d49779e9f7746c9bc1ec4ad9e7bc1d01ab58b85f9154`
- TekesClientKit `Sources/Composer/Agent/ClientAgentSummaryEnvironment.swift` — `d325617e6b56472231a06b41152a8acde9ac85161fd588bec63e4e462e6186ad`
- Tekes host `Tekes/ClientAgentSummaryHostAdapter.swift` — `4e7875ae43ab06901a981077f81a4504c754e88f2c019a470dc67222b917714a`
- AgentKit `crates/presentation/src/summary.rs` after this change — `7545efff1524342c4b6e1c391ca181dfae28d91a52bdbc3bdf13d639ea89026f`

以上为当前 checkout 的静态代码证据，不代表运行中的应用版本，也不证明所有 harness 的 producer 都产生了完整 Summary 数据。缺失的 producer facts 继续保持 unknown；未发现需要补造 SDK/TekesUI API 的阻塞。

## 修改与验证范围

- `crates/presentation/src/summary.rs`：subagent session 候选/授权分离，inventory membership 与唯一后缀匹配。
- `crates/presentation/tests/state.rs`：Pi 子代理无 store 时被动；将同一投影和已知 child 放入 store 后才产生动作。
- `crates/presentation/tests/panels.rs`：无 store 被动、known session、unknown UUID、exact 优先于其他后缀、唯一/歧义 endpoint-qualified 后缀、raw/namespaced parent 自身与跨 endpoint 防护。

未运行 Cargo、harness、桌面或 TekesUI 测试；主会话集中运行 Rust 验证。`rustfmt --edition 2024 --check crates/presentation/src/summary.rs` 可作为单文件格式检查。当前结果不构成编译或测试通过证明。运行端到端集成前，仍需由 Native/Pi/DSH 的真实当前 session inventory 验证返回 ID 与 store key 的映射格式。

## 主会话集成验证

主会话已完成 presentation 全套测试：225 passed / 0 failed / 0 ignored，日志保留在 `.runtime-fixtures/parallel-issues-20261024/presentation-tests-final.log`。首轮发现的旧 fixture 预期和 section 下标问题已修正；独立审查发现的带前缀父 ID 碰撞已新增回归并通过。UI、公开 consumer 与真实 harness 的本批结果由主会话集中收据另记，不把此前冻结程序的结果转移到本修复。
