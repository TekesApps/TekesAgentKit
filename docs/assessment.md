# TekesAgentKit 初步架构评估

> 历史评估。仓库后续已公开并采用 MIT License（v0.1.1）；本文的私有/许可证待定描述仅记录评估当时状态。后续实现发现当前 ClientKit Native 已要求 Kernel v4 并移除 workspace RPC；本文的旧 Kernel v3/workspace 记录不能作为当前实现基准。见 headless-kernel.md。

评估日期：2026-10-17。以下为源码核对与设计建议；没有进行 harness 连接、性能基准或原生 UI 验收。

## 1. 结论与边界

可行。建议将 TekesAgentKit 定位为 ClientKit 的 Rust 对等套件，复用协议语义、用户行为和验收用例，
不逐行翻译 Swift/AppKit，也不把 Kernel 的内部 crate 当作客户端 SDK。

SessionEndpoint 是连接边界；Kernel、Pi、DSH 是 harness adapter。它们不应各自带一套 UI 或状态机。
同一 harness 可有不同部署方式，部署位置不等于协议或存储能力。

第一版不更换现有 Swift ClientKit，不修改 Kernel，不迁移现有应用。本轮只建立编译骨架与评估材料。

## 2. 已核对的现状

| 来源 | 当前证据 | 对设计的影响 |
|---|---|---|
| ClientKit Package.swift | Contract / Session / TranscriptRendering / TranscriptCoordinator / ComposerKit / ToolPanelKit 已分层 | Rust 保持 contract、session、presentation、components 的方向边界 |
| Contract/SessionEndpoint/SessionEndpoint.swift | 五类同步流，historyPage、actionable 响应和独立可选能力协议 | 核心 trait 保持精简，model/management/files 等扩展独立；不要巨大 trait 全部必实现 |
| NativeSessionEndpoint.swift | launcher 提供 URL/凭证，客户端连接 remote.mux，处理 WebSocket 与 unary 调用 | 本地 Kernel 当前仍可通过 loopback 接入；“local”不自动意味着进程内调用 |
| PIHostEndpoint.swift 与 PI/Resources | Pi 接入包含进程、RPC、投影及受管理扩展 | 不是换 URL 就完成；先界定最小兼容集，再移植扩展和 supervisor 生命周期 |
| DSHHostEndpoint.swift | 使用 DSH gateway 并映射 SessionEndpoint 能力 | 不假定 DSH 原生支持 Kernel 的所有路由/扩展 |
| SessionEndpointRuntime / SessionWindowCoordinator / SessionEventProjector | 生命周期、窗口与事件折叠独立，但通过 Store 写投影，流式 transient 有独立内存路径 | 可保留语义并更换存储实现；不能直接删除 Store 调用就认为状态一致 |
| SessionEndpointStoreContract.swift | Store 是可替换的异步协议，值类型仍带 Mirror 命名 | Rust 可明确命名为 SessionProjectionStore，持久缓存另设接口 |
| Tekes/ClientAgentHostAdapters.swift | Host 通过 environment/actions 接入共享 Run controller | Rust 同样通过输入/回调接产品能力，Host 不复制组件状态 |
| ClientEmbeddedRunView.swift | task/subagent/SideChat 复用同一 Run controller，按 scope 区分 | 一套 RunSurface 加 scope；生命周期与草稿隔离必须先约定 |
| ClientStandaloneComposerController.swift | 当前未提交 WIP：document scope、共享草稿、submission reconciliation、IME 生命周期 | 记录为候选新行为，不能当已发布 API；落地前重新比较 |
| TekesUI 主 checkout | Cargo workspace 当前只有 motion crate，且尚未提交 | 不能据此判断完整 UI 能力不存在 |
| TekesUI gallery-design-baseline worktree | 有 controls/tokens/scene/render/text/platform 等 crate，当前大量 WIP，含 composition API | 可作为后续对接来源，但尚不能静默绑定工作目录或宣称 API 稳定 |

准确 HEAD、dirty 状态和所读文件 SHA-256 见 source-baseline.json。HEAD 不包含 WIP；哈希仅标识本次所读内容。

## 3. Local 不用 Mirror：可以，但先明确 Mirror 的含义

建议区分三类状态：

1. **Harness authority**：会话、消息、执行、队列与审批的权威记录，由 harness 负责持久化和恢复。
2. **Session projection**：客户端统一内存读模型，含 inventory、已加载历史窗口、运行中增量、能力与连接状态。
3. **Persistent replica/cache**：可选客户端磁盘缓存，负责离线阅读、启动首屏和历史检索加速。

如果“不用 Mirror”指不复制写入客户端 SQLite，可以；如果指 UI 直接查询 harness 私有数据库，或完全不做
事件投影，则不建议。Local Pi 仍可能是独立进程，Kernel 本地也有连接中断和重启，仍要做序列校验、
baseline、窗口合并、transient 替换、乐观输入去重和通知。

建议热路径：

```text
Harness → Endpoint adapter → Runtime / reducer → Memory projection → Presentation → Components
                                              └→ Optional durable cache (bounded batches)
UI intent → Runtime command → Endpoint adapter → authoritative receipt / stream reconciliation
```

UI 热路径不得等待逐 token 磁盘提交。耐久事件/缓存可批量写入；缓存写失败应降级为可见的 cache degraded，
不能把已被 harness 接受的命令误报为未接受并自动重发。Harness 自身权威记录的持久化不在此优化中删除。

| 情况 | 建议默认 | 代价/约束 |
|---|---|---|
| Local，支持完整 inventory/history 和事件恢复 | MemoryOnly | 重启从 harness 重载；暂时不能离线阅读 |
| Local，大历史或要求冷启动首屏 | DurableCache 可选 | 需测量磁盘开销、缓存容量和失效成本 |
| Remote | DurableCache | 断线保留已读内容并标为 stale；缓存不是服务器权威 |
| Remote，临时/敏感会话或禁用落盘 | MemoryOnly 可选 | 断网无持久历史，需明确状态 |
| Harness 缺乏持久恢复能力 | 先补 adapter 的恢复契约或限制能力 | 不能把本地等同于天然可恢复；adapter journal 如承担权威责任就不是可丢弃缓存 |

因此将 TargetDescriptor 的 harness kind、connection locality、cache policy 分别建模。具体枚举和 trait 签名
留到协议评审，不在本轮固定。推荐默认不等于所有 local 永远禁止缓存、所有 remote 必须落盘。

### 一致性要求

- 地址至少包含 endpoint identity + session identity；同名本地/远端会话不能互相污染。
- 每个逻辑流接受一次 baseline，绑定连接 generation；过期 reader/响应不得更新新连接状态。
- journal follow-first，在 throughSequence 固定切点分页；新消息不能改变旧页的时间范围。
- gap 或内容冲突触发受控重新开流，禁止跨缺口盲目继续；重复事件按契约检查。
- 窗口 snapshot 只替换其覆盖范围；hasMoreBefore 时不能把未包含的历史当作删除。
- transient 不推进 durable cursor；权威消息/rebaseline 能取代 provisional 内容。
- actionables 的 revision 与失效由权威流确认；不能从磁盘缓存恢复成可直接批准的活动请求。
- prompt acceptance、queue admission、最终 transcript reconciliation 分别建模，避免重复提交。
- 服务端状态与用户草稿、滚动位置、折叠状态分开存储；MemoryOnly 不自动禁止用户偏好的持久化。
- cache key 必须区分服务身份与账户作用域，凭证不得写入缓存；退出/切换身份的清理规则需单独定义。
- 远端断线时禁用需要新鲜 authority 的操作；缓存内容保留并明确 stale，不伪装 ready。

### 速度判断

去掉客户端磁盘复制可能减少写放大和同步等待，但还不能承诺更快。当前 Swift 已有 transient 内存通路，
收益取决于持久事件频率、事务、全量重建和 UI 失效范围。后续用同一回放分别测 MemoryOnly / DurableCache：
首事件到可见帧 p50/p95、冷启动、10k+ 历史分页、长输出帧时间、RSS、磁盘写量和断线恢复。
预先固定数据集、机器、构建模式与采样方式，再设门槛；本轮没有性能数字。

## 4. 建议 crate 边界

| crate | 职责 | 不应拥有 |
|---|---|---|
| tekes-agent-contract | DTO、事件、能力与 SessionEndpoint 抽象 | UI、磁盘数据库、Kernel engine |
| tekes-agent-endpoint-kernel | 原生 wire/client carrier | Kernel 内部执行状态 |
| tekes-agent-endpoint-pi | Pi RPC、扩展桥接、事件规范化 | UI、通用 reducer 的分叉 |
| tekes-agent-endpoint-dsh | DSH gateway、版本/能力协商、事件规范化 | 把缺失能力伪造为成功 |
| tekes-agent-session | registry、生命周期、投影、分页、恢复、缓存策略 | TekesUI、窗口和产品导航 |
| tekes-agent-presentation | Run/Composer 等展示状态、intent、scope/lifecycle | 平台窗口、数据库 schema |
| tekes-agent-components | 基于 TekesUI 的 Agent 业务组件 | 通用 UI 基础设施、宿主产品场景 |

缓存后端将来可拆 tekes-agent-cache-sqlite；暂不为尚未确定的存储选型增加 crate。
骨架依赖只表达方向，尚未定义 trait 或接入 TekesUI。具体异步 runtime/HTTP/WS/SQLite 库在实现前选择。

Kernel 当前内部 crate 使用 schema/endpoint 等通用名且 publish=false。不要直接把 server crate 加入客户端。
协议事实以 ClientKit contract/fixtures 和 Kernel 接受的 authority corpus 核对，Rust DTO 可自行实现或后续生成；
生成方案需保证缺失/null、整数范围、未知事件/能力、错误结构和 canonical fixture 语义，不只比较字段名。

## 5. UI 对齐：命名为 Components

建议套件叫 AgentKit，代码层叫 Presentation 和 Components；Components 是有 Agent 业务语义的组合界面。
TekesUI 的 controls/composition 仍承担通用控件与组合基础，两层不重复命名所有权。

| ClientKit 对齐面 | Rust 组件建议名 | 必须对齐的行为 |
|---|---|---|
| RunView / ClientRunViewController | RunSurface | 同一会话的 transcript、composer、loading/error、scope 生命周期 |
| TranscriptRendering + Coordinator | TranscriptSurface | 稳定消息身份、分页、虚拟化、选择复制、代码/工具、滚动锚点与 follow-tail |
| Composer / UserControls | Composer | 草稿、IME、撤销、附件、模型/预设/权限、queue/steer、提交回执与回填 |
| StandaloneComposer WIP | StandaloneComposer | 文档作用域、共享草稿、annotation、提交和 transcript 的确认；发布前重核 |
| StackedActions / QueuedInput / Approval | ActionablePanel / QueuePanel | revision、过期响应、失败恢复、自定义输入与审批状态 |
| Agent settings / provider controls | AgentSettings | 能力驱动可见性、配置写回、secret 由 host 管理 |
| Task / subagent / SideChat | EmbeddedRunSurface | 复用 Run 状态机；父子作用域、独立草稿、临时分叉清理 |
| AgentSummary | AgentSummary | 汇总事实与导航意图分离，host 提供打开动作 |
| AppServerHealth / status ticker | ConnectionHealth | connecting/ready/failed/stopped；live 与 cached/stale 分开 |
| AgentSidebarList | AgentThreadList | inventory、未读、归档/改名、加载与折叠状态 |
| ToolPanels | ToolResultSurface（后续） | 结构化结果与 host 工具面板适配；不能复制终端/浏览器平台基础设施 |

本轮只是逐面设计映射，不是完整字段/交互 parity 审计。实现前每面建立“设计契约 / 代码接入 / 实际体验”矩阵，
分别标记通过、失败、未测、不适用。不能把 Rust 编译、fixture 通过或静态截图当作 IME/a11y/120Hz 验收。

## 6. TekesUI 对接风险

完整工作树有通用文本、滚动、选择、菜单、dialog、composition、平台与渲染模块，方向上适合作为基础。
但存在大量未提交变更，且主 checkout 与活跃 gallery 树不同；AgentKit 本轮不引入 path 依赖。

在第一个业务组件之前，验收：多行输入/marked text、选择与撤销、稳定焦点和视图身份、长文档虚拟化与锚点、
异步局部更新、平台 accessibility、弹层、主题/token 输入与 reduced-motion。模块存在不代表这些能力已完成。
缺失的通用能力应回 TekesUI 实现，Agent 专有行为放 AgentKit，禁止 host 各自补一个状态机。

工具链也需显式处理：系统默认 rustc 为 1.83；本机已安装 1.95.0。活跃 TekesUI 要求 1.95，Kernel 要求
1.95.0；新仓库固定 1.95.0。尚未验证其他操作系统；Rust 分层不等于全套组件已跨平台。

## 7. 尚待实现前确定

- ClientKit 基准版本与哪些 Composer WIP 纳入第一版；不能从未提交代码宣称发布级 parity。
- Pi/DSH 对应版本、接入与扩展支持矩阵；以实际握手、fixture 和测试建立，不凭 harness 名称推断。
- TekesUI 不可变依赖版本及通用能力验收结果。
- 最终缓存后端、容量/清理策略、离线产品契约与性能门槛。
- Rust host 和第二个独立 public-API consumer 的具体选择。
- 仓库开源/许可证与 crate 发布计划；当前私有、publish=false。

这些不阻碍本轮骨架和评估完成；不能据此提前宣称端点或 UI 可用。
