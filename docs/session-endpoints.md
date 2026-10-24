# 三个独立 SessionEndpoint 的无 UI 闭环

日期：2026-10-17。改动只在 TekesAgentKit；ClientKit、Tekes、Kernel、Pi、DSH 作为只读来源和运行时。

2026-10-22 补充：[目标终态只读投影](rust-agent-ui-goal-footnote-20261022.md)从现有公开 goal projection 读取 scope/identity/phase，随原 RunSnapshot 发布；没有新增 RPC、mutation 能力或 harness-private 读取。共享 API 尚未发布，当前三端正常 GPU 回归不等同三端目标专用验收。

## 契约与路由

`NativeSessionEndpoint`、`PiSessionEndpoint`、`DshSessionEndpoint` 分别实现 `SessionEndpoint`。
Native 要求 Kernel 0.9 的 v4 wire；Pi/DSH 的标准化 HostDescription 保留 ClientKit v3 描述，**不表示它们使用 Kernel v3 wire**。
连接身份、transport locality、harness、cache policy 相互独立。

`SessionEndpointRegistry` 注册显式 connector。`SessionAddress { endpoint_id, session_id }` 是跨 harness 的稳定路由键。

2026-10-20 补充：[评价读取与身份契约](rust-agent-ui-feedback-read-20261020.md)统一三端 committed feedback 数据。DSH 的公共 feedback.list/put/delete 现在在适配器中解包成功结果、将内层失败作为错误返回，并保留 CAS conflict current；使用这些公共别名的调用方不应再次按 DSH value 解包。该修正已有适配器单测和真实三端只读验证。后续[CAS 写入验证](rust-agent-ui-feedback-cas-20261020.md)补齐三端真实评分/备注/冲突/删除闭环；评价写入控件仍未开放。
`SessionLocator` 给出该地址、cwd、nativeSessionId、可用时的 nativeSessionFile，以及 historySource。
`cwd` 保留原始工作区根，`workingDirectory` 是有权威投影时的当前执行目录；两者可以不同，文件相对路径仍遵循各服务端的 scope。
原生文件指针是定位证据；历史读取仍通过对应 harness 的公开协议，不解析私有数据库或把远端路径当成本机路径。

`SessionRuntime` 合并 inventory/control/actionables/workspaces 和只观察的 endpoint events，按需打开会话窗口，提供订阅、snapshot、前页、定位和只读文件 API。
慢 unary / prompt / actionable 响应以最多 64 个在途操作运行，流仍能投影；重连会中止旧连接操作，不自动重发 mutation。
`Earlier` 的网络取页仍串行，底层 transport 保持有界读取。主连接与单会话流均有有限恢复预算。

## 能力矩阵

| 能力 | Native | Pi | DSH |
|---|---|---|---|
| 生命周期 | 显式 URL / bearer，host 启动 Kernel | 显式可执行文件/参数/env，每会话一个 stdio 子进程 | 显式 URL / cookie 或 header，host 启动 DSH |
| 版本证据 | v4 host description | CLI `--version`，只接受配置允许版本 | `/api/tekes/harness-version` 官方 runtime version probe |
| 会话 inventory/control | 权威原生流 | 宿主指针 inventory；原生 state 保存在 conversation；共享 control 基线为空 | `$events` + 分页 list；session/control |
| 创建与模型 | 原生 unary | 显式 cwd；原生 model catalog / set_model | session/create；session/modelCatalog、模型选择 |
| Prompt/取消 | 原生 unary / rpcId | text/image/file、持久化 queue/steer、abort | session/prompt、cancel |
| Prompt 重复提交 | 服务端 rpcId | 磁盘 receipt：pending 先落盘、ACK 后 accepted；不确定结果拒绝自动重发 | 服务端 requestId |
| 历史 | journal / frozen cut | get_entries，按 leafId/parentId 重建当前分支 | follow/page，展开 packed chunk row，保留 seq/time/tool view |
| 前页 | 固定 throughSequence 的页 | 扩大原生 ancestry 窗口并刷新 | 固定 throughSeq 的页 |
| 实时数据 | durable event + transient preview | native-event 有界队列 + 关键事件刷新原生 conversation | durable event + assistant-stream revision/index 校验 |
| 实时 output tokens | 透传 provider 的 `assistant/chunk` usage 累计报告 | 当前 conversation 路径无实时 usage，保持缺省 | assistant-stream 原样透传 usage chunk；仅上游报告时可用 |
| 每步结算与历史 output tokens | 真实消息 / 回合 usage 投影 | 未提供时保持缺省 | 公开 follow/page 的消息与完成 stream usage；统一 reportedUsage，支持恢复和分页覆盖标记 |
| Actionables | 权威 sync/revision | 原生 confirm/select/input/editor 转换，回答绑定原进程 | waterfall clientId/eventId 关联及取消；inventory 流不抢答 |
| Rename/archive | 原生调用 | 原生 rename；宿主 archive/unarchive 指针 | 原生 rename/fork/archive/unarchive；fork 清理继承的未消费 inbox |
| 既有 session | 服务端 inventory | 显式 `session.adopt`，原生 RPC 验证文件；不全盘扫描 | 服务端 inventory |
| Session 工作区文件 | session.files.* | cwd 内 capability 限定只读文件 | workspaceFiles，二进制 multipart |
| fork / discard | 原生普通/临时 fork、临时 discard | 原生克隆/精确 entry fork；临时分支及 discard | 普通 fork；临时 fork/discard 明确 Unsupported |
| 工作区 | v4 未声明 workspace-sync | 宿主工作区与移动/重命名、会话关联 | 原生 workspace/follow，replace/reorder/archive/pin |
| 附件 | 原生策略、文件上传及会话内字节读取 | 限额校验、图片/文件上传、会话隔离及历史附件读取；普通文件在 prompt 中转为路径引用 | 原生策略和 raw binary 文件上传；历史保留文件元数据；公开 attachment 只读图片，无通用文件下载 |
| 文件观察 | 认证 WS changes | 有界轮询 ready/change/absent | workspaceFiles/changes |
| 扩展 | 服务端公开方法 | 模型/命令/skills/tools/settings 读取、Goal/反馈 CAS | ClientKit 对应公开服务路由及规范化参数 |

Pi 的关键事件刷新在同一 `conversation-update` 帧中携带 `snapshot` 和可选的原始 `event` 原因。投影先验证两者，再一次发布；`message_end` 可以结束 assistant 临时预览，而不把仍在工具阶段的整个 Agent 标为完成。旧的无 event 帧仍可读取；新字段不生成 journal seq 或伪造消息 entry ID。相同 timestamp 不再作为临时回复覆盖持久消息的依据；完整 native 流/记录阅读身份迁移仍另行验收。
| 子代理 | 原生 catalog；扩展只读子历史；bounded child 控制不可用 | create/list/prompt/read/cancel，官方扩展模型工具 | projection catalog + 父拥有的 prompt/cancel；child 地址历史与重连 |
| 产品服务 | 服务端或显式宿主能力 | 显式 HostServices | 服务端部署或显式 HostServices |

公开类型化 API 提供 create/submit/fork/discard/rename/archive/unarchive/queue/upload/goal；其他公开扩展经能力检查后调用。`EndpointCapabilities` 区分方法、归档、临时 fork 和文件观察；不支持的方法明确失败。
Pi 不提供虚构 journal、durable cursor 或 task/job。队列是宿主持久化的提交意图，支持编辑/移除和结算后自动派发；超时不重试不确定的派发。旧进程事件不能驱动新进程。
Runtime 在 archive/discard 前退订对应会话内容流，失败则恢复；其他会话保持连接。archive 只隐藏指针，不删除原生文件。
Pi Goal 支持 CAS、暂停/恢复、原生工具记录完成/阻塞及有界自动执行；每轮先持久化预算，达到上限后停止。激活只存在于当前连接，重连不会自动继续。反馈以原生 entry ID 和版本进行 CAS。
Pi 使用小型 JavaScript companion 调用 Pi 官方 extension API（fork、设置/工具读取、自定义元数据），没有 Swift、私有 RPC 或日志解析。凭证、安装、Git、系统权限及产品插件后端由 `HostServices` 显式注入；只增加宿主声明的方法，不伪造产品实现。默认 Pi preset 明确限定，未知 preset 被拒绝。
Endpoint events 是有界观察数据，不会变成 actionable authority；DSH 观察 waterfall 显式 next，不抢占交互处理。
DSH version probe 需要宿主提供和 ClientKit 相同的版本桥接；测试用临时 JS plugin 调用官方 `getDshRuntimeVersion`，不信任自填版本。

## Session 指针与文件

Pi 数据根由宿主显式配置，注册表 `agentkit-sessions.json` 保存稳定地址到原生文件的映射与提交 receipt。
注册表用私有文件权限、临时文件、fsync + rename 持久化。它只保存指针、工作区、队列提交意图和 Goal/反馈元数据，不复制 transcript。数据根独占锁防止双连接覆盖；重启清理宿主拥有的临时分支与附件。
`session.adopt { cwd, sessionFile, sessionId? }` 只接受 `PiConfiguration.session_roots` 显式授权目录下的现存文件；
通过 `get_state` 验证实际恢复的文件，后续历史仍使用 `get_entries`。注册表重启时也校验授权根。

Native/DSH 的会话存储格式和物理日志路径由服务端拥有；不直接读取内部 ledger/JSONL。
Pi 可通过 locator 返回实际原生文件路径，并通过 `session.history.export` 返回原生 entry tree；它们不混同于 cwd 文件资源。

文件 API 提供 stat、按行 UTF-8 预览、按字节分页及 `file_read_all(maximum)`。
完整文件获取逐页校验路径、版本、总长、offset、base64、eof 和进度；文件改变就失败，避免把不同版本拼起来。
Pi 用目录 capability 打开文件，拒绝 cwd 外路径和 symlink 逃逸；文本预览限 4 MiB，单字节页限 4 MiB。
Native/DSH 的授权和文件版本由服务端决定；DSH multipart 校验部件、重复 slot、codec 和边界。

## 验收证据

三个真实运行时在临时 HOME / 独立数据目录及本地合成 provider 上均通过：
connect → list → create → select model → open → prompt → assistant 回传 / settle → 同 rpcId 重试 → locator → stat / text / bytes → reconnect / reopen → 历史保持。
检查实际用户输入只有一条；DSH runtime-context、time-context 和 working-directory 的 user/message 按 source 区分，只有 source.kind=user 计为用户提交。

Pi 额外通过原生 extension command 等待 select、在 prompt 尚未完成时回答、答案写入 native entry，
再复制该临时测试 session 到显式授权目录，用新的指针注册表恢复并读取原历史。
Kernel 另有 frozen-cut 分页和取消后的第二次 turn/end 验收脚本。

Rust 回归覆盖契约 fixtures、原生分支/坏树、文件路径与分页、DSH packed chunks / assistant gap / multipart，
共享 native conversation 与 journal 隔离、重连不重试 mutation，以及 pending prompt 不阻塞 actionable response。
依赖边界检查覆盖 contract/session/三个 adapter/headless，拒绝 UI、SQLite、服务端内部与隐式 sibling 依赖，并禁止 Swift 源码。

这些证据不代表真实付费 provider、TLS 远端部署、生产 launcher、大历史/RSS/吞吐、Linux 或 UI 的验收。
远端 durable cache、完整 UI projector 和产品专属后端不属于本轮非 UI SEP 交付。

## DSH 0.2.1 升级验收

当前固定版本为 **0.2.1-rc.2**（官方 tag `dsh-v0.2.1-rc.2` / `687f282e8625c6b8d11ff58211a90524be2e4317`），同时保留 0.2.0-rc.2 兼容。
版本 allowlist 不接受未验证的后续版本；读取到官方 runtime 证据才允许连接。安装器、source archive、npm integrity、runtime/lockfile 哈希见源码基准文件。

- 新增 log-only `job/registered`、`job/settled`、`working-directory/change` 被明确登记并无损保留。它们是从 DSH 新目录核对出的契约扩展，不伪称已在当前 ClientKit 的旧事件表中。
- 未知必需事件仍拒绝；实验性 `plugin:` ignorable 记录保持原始载荷。
- `session/projections` 的 `kind=sequenced` 才是带切点的权威值；`migration-required` 的缓存提示不用于当前工作目录。inventory 的 formatStatus 仍保留。
- session/discard 在两个官方验证版本均未提供，明确 Unsupported。ephemeral fork 会被拒绝，避免留下无法丢弃的耐久会话。
- 普通 fork 与 ClientKit 一样尝试移除继承 inbox；失败时返回已创建的 sessionId 和 queueCleanupWarning，不隐藏新指针、不自动重试 fork。

**两个版本均已真实验证**：基础回传/重连/文件链路，rename、普通 fork 及子会话历史、cancel、archive/unarchive；28 次合成 turn 后以新客户端打开裁剪窗口，再取前页且冻结切点不变。
**0.2.1-rc.2 额外真实验证**：合成 provider 触发官方 working_directory 工具，把 cwd 切到测试子目录，观察新事件、独立的原始根/当前目录，并重连保持。
Pi 1.3.0 和 Native Kernel 0.9.0 的原验收也重新通过。上述模型输出均来自本地合成 provider，没有请求 DeepSeek 或其他真实付费模型。

## 本轮新增验收

三个真实版本均通过模型/命令/skills/设置读取、附件上传与文件观察的 ready 握手。Native 验证临时 fork/discard、归档恢复；Pi 额外验证精确原生 entry fork、临时分支实际删除、工作区同步、队列编辑/移除/自动派发、附件隔离、Goal/反馈 CAS，以及模型完成、轮数预算耗尽、重连不激活。DSH 验证服务端扩展、含加号文件名上传和工作区同步。

42 项 Rust 回归通过 fmt、Clippy -D warnings 和依赖边界检查；回归另外覆盖能力和显式宿主服务边界、文件观察变化/删除/关闭、工作区基线替换/排序、观察事件有界与非权威、活动内容流的 discard。`tekes-agent-headless` 是独立的公开 API consumer，与 adapter 一同构建并执行上述联调。合成 provider 证明协议闭环，不代表真实模型质量或生产部署验收。


## 2026-10-18 对照复查

当前 ClientKit 的 SEP 参考文件哈希均与基准一致（HEAD `c289bc9`）。复查修复了 Pi 原生文件缺失/替换后的恢复保护、ClientKit `atSeq` 完整 entry 顺序及已完成 turn 校验、失效 pointer 的单会话隔离，以及 Native/Pi 在无文件观察者时的 close 状态持久化。详情及验证边界见 [复查记录](sep-review-2026-10-18.md)。

附件组件的三端闭环收据见 [上传绑定验收](rust-agent-ui-upload-three-20261020.md)。Native/Pi 的通用文件字节回读及跨会话拒绝已验证；DSH 普通文件只验证上传、历史元数据与发送回传，不将图片接口或 workspace 文件 API 冒充任意上传附件下载。

## DSH actionable 回答信封对齐（2026-10-20）

只读对照当前 ClientKit `DSHHostEndpoint.remoteOutcome` 与
`TekesClientRuntime.respondWithReceipt`，修正 Rust DSH 将共享回答信封直接交给
waterfall handler 的差异。Actionable correlation 保留注册 kind：question 的
`{sessionId, answer}` 只提取 answer；approval 的 `{sessionId, approvalId, outcome}`
只提取 outcome 词。已是原生 DSH 形状或未知 kind 的值保持原样，不按 UI 文案猜类型。

独立 DSH 0.2.1-rc.2、本地合成 provider 和临时数据根验证：模型实际调用
ask_user_question → public actionable sync → runtime 用共享 session envelope 回答 →
答案进入无错误的 tool/result → settle、actionables 清空。脚本检查返回给模型的结果，
不只检查 RPC 返回。DSH 其他生命周期/历史/文件/工作目录回归同轮通过。

审批两种词及 native/unknown/wrong-kind 的无损行为有单元测试；本轮未触发真实审批、
未做原生 actionable UI。后续 UI 必须消费共享回答契约，适配差异留在 SEP。

本轮本地 gates：root workspace 108 tests、DSH 9 tests（包含在 root 总数）、18 项工具测试、DSH all-targets strict Clippy、fmt 与 headless 依赖边界通过。此为协议增量，不作为 UI/物理 IME 的新证据。


## 共享只读会话搜索

`SessionRuntime::search_sessions_at_epoch` 基于真实方法能力调用 `session.search`，并在请求入场和回复时校验连接代数。typed `SessionSearchResults` 要求 items/sessionId/snippet/hasMore，保留未知扩展字段，不将缺少 hasMore 当成已穷尽。Pi 返回至多 100 个匹配并以第 101 个匹配证明 hasMore；引用候选接口仍为 20 条。DSH 全文索引由部署决定，默认关闭时保持明确失败，AgentKit 不自动打开它。读取取消只撤销 search reader，不取消远程 Agent 执行。组件、shared presentation 和分项证据见[接入记录](rust-agent-ui-sidebar-query-controller-20261021.md)。


搜索遇到未知库存身份时，共享运行时通过 `refresh_inventory_at_epoch` 重读公开 session/workspace cursor 的原子 baseline 对；原连接 epoch、已加载 journal/preview 和分页 cut 保持，旧 cursor 被淘汰。刷新失败保留原库存并向搜索传播失败，不伪造空命中或偷偷重连。实现与验收范围见 [Sidebar 库存恢复](rust-agent-ui-sidebar-inventory-refresh-20261021.md)。

## 2026-10-22：当前运行版本与原生UI证据

见[三端原生验收](rust-agent-ui-three-real-20261022.md)：Kernel0.9.3、Pi1.3.0、DSH0.2.1-rc.2在固定09b SDK及同一冻结UI程序下通过公开分页、流式阅读、合成中文预编辑、取消/错误恢复和重连。Native0.9.3完整协议流程的archive返回session-running，仍未完成其全量生命周期验收；独立UI模式不代替这项失败。脚本现在支持明确supervisor/worker路径、build哈希及精确版本/握手检查，旧target中的0.9.0开发程序不再被按目录名计作发布版。能力声明与单次运行验收结果保持区分。

## Sidebar 分类事实（#29，2026-10-24）

分类只决定 Sidebar 成员，不授予子会话继续运行或历史读取权限。当前 shared predicate 排除明确 origin=subagent 或 ephemeral=true，parentSessionId 单独存在不排除 fork。缺失/未知数据保持兼容，但不能从兼容显示反推已确认 main。

| 来源 | 已核实的公开 producer | origin / parent | ephemeral |
| --- | --- | --- | --- |
| Native SEP v4 / Kernel v0.9.3 | 公开 tag 解引用 `09508a9083c24c7accefc99516739f279b85913e`；规范与 supervisor inventory_row | 完成的 fork 报 origin=fork 与 parent；普通行省略，不合成 subagent | genesis flag 为 true 才报告，否则按该固定规范省略 |
| Pi | 本仓 `endpoint-pi::summary` 的规范化公开 producer | subagent_parent 产生 subagent；普通 parent 只保留 lineage | 规范化 row.ephemeral 布尔值 |
| DSH 0.2.1-rc.2 / 0.2.0-rc.2 | 固定官方源码 `687f282…` / `639ed015…`；SessionSummary、在线/冷会话 summary、listFields | 显式 header.origin=subagent 原样透传，parent-only fork 不推造 origin；旧/迁移行的缺失仍非独立身份权限证明 | 两版公开 SessionSummary 没有该字段；临时 fork/discard 仍按既有能力明确 Unsupported，不补 ephemeral=false |

Native 公开来源：[固定规范](https://github.com/TekesApps/TekesKernel/blob/09508a9083c24c7accefc99516739f279b85913e/spec/session-endpoint.md#read-routes)、[固定 producer](https://github.com/TekesApps/TekesKernel/blob/09508a9083c24c7accefc99516739f279b85913e/crates/supervisor/src/endpoint_carrier.rs)。DSH 公开来源：[0.2.1 list producer](https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/packages/api/session-controller/src/list.ts)、[其 SessionSummary](https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/packages/api/session-controller/src/types.ts)、[0.2.0 list producer](https://github.com/deepseek-ai/deepseek-harness/blob/639ed015397290b3745d163aafe02ffee4aa3f84/packages/api/session-controller/src/list.ts)。精确版本、Git blob 哈希与协议回归分层记录在各 wire 专题；源码核对和本地协议 mock 不能替代已安装 runtime 身份或真实 harness 整体验收。
