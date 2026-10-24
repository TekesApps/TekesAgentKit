# Sidebar 多 endpoint 聚合设计草案

状态：Issue #22 的 additive binding slice 已提交 `6c700e9`；主会话完成本批统一验证，完整 Issue 仍未完成。本文只界定 Rust AgentKit 的多 endpoint 数据与路由边界；不修改 TekesUI、SDK、现有单 endpoint API 或 Sidebar 状态机。

## 背景与边界

现有 `SidebarBinding::new(agent, endpoint_id)` 是一个合法、明确的单 endpoint API：它观察传入 endpoint 的 inventory，并用同一 endpoint 处理选中行补读和共享 thread action。单 endpoint 使用者不需要为此迁移。

当前原生 demo 也只创建了单 endpoint Sidebar：`ui/demo/src/components_native.rs` 以 `scopes[0].endpoint_id` 创建 `SidebarBinding`，随后把它同时设为 selected reader 和 inventory source。若一个 AgentKit 宿主配置多个 endpoint，这意味着 demo 尚未把其余 endpoint 接入 Sidebar；这是宿主装配覆盖范围，不是 `SidebarBinding` 把操作误路由到别的 endpoint。该单端 API 收到 endpoint 不匹配的行时会拒绝，不会静默改投其他端点。

ClientKit 的 `ClientSessionSidebar.sessionRows` 从宿主提供的全局 `sessionThreads` 与 `globalThreadSnapshot` 生成会话行，路由身份仍由各行事实携带的 endpoint/workspace 信息决定。Tekes 宿主持有各后端和 session 导航；共享 ClientKit 管理可复用呈现。Rust 侧相应地由宿主明确提供要汇总的 endpoint 集合，bindings 负责观察、身份校验及按行来源路由，组件只接收投影后的行和选择/筛选/分页输入。不得把多 endpoint 生命周期或 SDK 调用塞入 Sidebar 组件。

## 最小公开 binding 方案

新增一个多 endpoint 聚合 binding（以下名称仅为草案，不代表现有 API；实施时按公开 API 命名约定确定），构造参数是同一个 `SharedAgent` 和一组固定 endpoint ID。保留 `SidebarBinding` 的现有构造与行为；多端宿主显式改用聚合 binding。聚合器内部为每个 endpoint 持有独立的既有单 endpoint observer/binding，并合并其最新 publication。

聚合 publication 可继续满足 `SidebarInput` 的现有消费形状：合并后的 `rows`、按完整 `SessionAddress` 索引的 activity、一个聚合 revision、一个 loading/error 投影。另提供按 endpoint 的健康/基线状态，避免宿主把局部错误误读为全局空列表。新增 binding 的公开面应以 snapshot/subscribe、按 address 读取 selected row、按 address 执行 action 为核心；无需给 UI 新增端点专属控件。

聚合构造必须拒绝空集合、空 endpoint ID、重复 endpoint ID，以及不属于同一 `SharedAgent` 的 endpoint 来源。构造时固定 endpoint 集合与顺序；运行中不隐式增删 endpoint。需要改变集合时，由宿主显式替换整个聚合 binding。替换后新聚合拥有新的 owner/epoch；旧 publication、读回执和 action token 均不能投影或提交到新聚合。

## 身份、读取新鲜度与操作路由

- 会话唯一身份始终是 `(endpoint_id, session_id)`，即完整 `SessionAddress`。相同 `session_id` 出现在不同 endpoint 时必须保留为两行；不得按 `session_id` 去重、覆盖或共享 pending 状态。
- 每个 endpoint 的 observer、连接 epoch、publication revision、error 和最后成功 rows 独立维护。聚合 revision 对每次可见合并状态变化严格递增，用作 Sidebar 输入版本；endpoint 本地 revision 不可相互比较或互相覆盖。
- selected read 依照目标 address 选择原 endpoint 的 reader，并以该 endpoint 当前 connection epoch、库存/会话版本和本次读取 generation 校验结果。读取 A 的回执不能补入同 ID 的 B 行；endpoint 重连、聚合替换、选择改变或源版本变化后，旧回执作废。各 endpoint 的 generation/freshness 相互独立，不能用一个全局计数掩盖旧回执。
- action intent 必须携带原行的 address 和 action definition。聚合器按 `address.endpoint_id` 选择既有 endpoint binding，再由该 binding 对当前共享定义/服务器请求进行验证和执行。endpoint 不在固定集合内、endpoint 与定义不一致、目标已失效或操作已变化时拒绝；不得按当前选中行、首个 endpoint 或重复 session ID 推断目标。
- copy action 返回的文本仍从所属 endpoint 的新鲜共享 action 结果读取；rename/archive/fork 等 mutation 也由所属 endpoint 发出。完成后的导航目标仍使用结果中的 endpoint 与 session ID，并由宿主决定是否打开。

## Ready、错误与局部故障

聚合不能把某个 endpoint 暂时不可用解释成空 inventory，也不能丢掉它最近一次有效 rows。

- 每个 endpoint 状态至少能表达 `loading`、`ready`、`stale/error`。首次加载前没有 rows；首次 observation 失败时保留错误状态而不伪造“已加载的空结果”。
- endpoint 已经发布有效 rows 后发生 observation 失败，保留该 endpoint 最后 rows，并将这些 rows 的 activity 标为 stale；成功 endpoint 继续接受更新。当前 `SidebarBinding` 的 observation error 会终止该 endpoint observer，本 slice 不新增重连状态机；因此 terminal error 期间不会由同一个 binding 自动恢复。宿主需在 endpoint 健康后整体替换多端 binding，新实例拥有新 owner token，旧读回执和 action ticket 随之失效。曾发布但本身仍带 stale activity 的 rows 即使没有新的 aggregate error 也必须保留 stale；旧 action ticket 在执行前还会重检 child publication，不能在 row 已移除或已 stale 后继续 dispatch。
- 聚合 `loaded` 只有在所有固定 endpoint 都有首个成功 baseline 或已明确进入首轮 terminal error 后才为 true；它只表示首轮结果已收敛，不表示所有 endpoint 都健康。任一 endpoint error 都必须可从 per-endpoint status 读取；现有单个 Sidebar error 字段可给宿主一个确定、带 endpoint 标识的摘要，但不能替代细节。
- 部分失败期间，合并 rows = 每个 endpoint 最近一次有效 rows 的并集。错误 endpoint 首次加载失败时贡献零行，但需保留其错误状态；曾经成功的 endpoint 断线时继续贡献 stale rows。当前 child observer 终止后没有同实例恢复路径，健康恢复需替换整个聚合 binding。
- revision 溢出按现有契约报错并停止接受新 publication；不得饱和后重复版本号。

## 排序、过滤、搜索、偏好与分页

- 排序对所有 endpoint 的普通 rows 统一执行，并以完整 address 作稳定 tie-break。置顶仍按共享 preference 的全局 pin order 呈现；相同 session ID 的不同 endpoint 仍是不同 pin identity。
- 文件夹身份沿用 endpoint-qualified `SidebarFolderKey`。同一路径在不同 endpoint 属于两个独立 folder；筛选只匹配精确 endpoint 与 folder identity。global pinned 区是否不受 folder filter 影响，沿用当前 Sidebar 组件契约。
- 普通筛选和分页发生在合并、排序后的列表上；仍使用当前本地展示分页，不为每个 endpoint 各显示一个“更多”，也不把它伪装成远端 cursor 分页。筛选/排序变化按现有组件契约重置页数；纯 inventory 刷新保留已请求展示条数；旧 page request 在组件/source 生命周期或筛选变化后失效。
- selected row 补齐先按完整 address 检查合并 rows；若缺失，再调用 address 所属 endpoint 的独立 selected reader。reader 返回的行必须重新通过聚合 owner、endpoint、generation、filter 和 search membership 校验。筛选挡住的行不应因补齐绕过筛选；查询结果投影仍不因 selection 任意制造普通命中。
- 现有 `SidebarSearchBinding` 绑定单一 runtime/endpoint。第一阶段可继续由宿主对每个 endpoint 分别提供已验证搜索 publication，再由聚合层按 endpoint 合并命中，并保留 endpoint-qualified identity；若没有这套多源查询输入，聚合 binding 必须明确只提供全局库存搜索能力之外的单 endpoint query，而不能把一个 endpoint 的 query 结果宣称为全局搜索。跨端搜索的结果顺序、部分错误和“全部结果已完成”语义应在实施前定约，避免把单端搜索 phase 直接复用成全局 ready。
- `SidebarPreferencesSnapshot` 仍由 SharedAgent 的现有共享偏好持有，不按 endpoint 复制写入。聚合 Sidebar 只消费同一份偏好，偏好 CAS/revision 冲突行为沿用现有契约；每个 endpoint 不得各自重置或覆盖该状态。

## Issue #22 的验收范围

Issue #22 中与 TekesUI #44 无依赖的多端聚合闭环，至少验证：

1. 两个 endpoint 同时发布 inventory，rows 合并；同名且同 `session_id` 的会话仍有两行，端点地址正确。
2. 选择非首 endpoint 的普通行和分页外 selected 行，补读只由所属 endpoint 完成；错误 endpoint、旧 epoch、旧 generation 的回执被拒绝。
3. 对两行分别发起 rename/copy/archive 或 fixture 支持的 action；RPC 到达对应 endpoint，延迟完成不会把导航或 selection 套用到后来选中的其他会话。
4. endpoint A 更新、断线、恢复时，endpoint B rows/revision 不被覆盖；A 的最后 rows 保留且 stale，首次加载失败不产生伪空成功状态，恢复后只更新 A 分片。
5. endpoint-qualified folder filter、合并列表排序、global pin、分页重置和旧 page request 拒绝符合现有组件契约；全局 preference 只提交一次且不发生 endpoint 间复制。
6. 单 endpoint `SidebarBinding` 的现有公开 API、现有消费者和测试继续通过；聚合 binding 的公开契约另由独立 consumer 覆盖。

以上为行为验收建议，不表示这些测试或能力已经实现。测试应使用两个隔离 fixture endpoint 和可控 publication/请求记录，不依赖桌面窗口。

## 验证边界与当前状态

当前实现新增 `SidebarMultiEndpointBinding` 和 `SidebarMultiPublication`，构造时固定并校验 endpoint 集合，组合原单端 observer publication；原 `SidebarBinding` API 与实现保持不变。selected read 复用单端 `SidebarSelectedRead` 并增加聚合 owner token。Action 先捕获完整 address/action definition 与 owner，再以 endpoint 当前 action catalog 复核并路由到原 child binding。全局搜索未实现，也不将现有 `SidebarSearchBinding` 的结果解释为多端搜索。原生 demo 仍只将 `scopes[0]` 接入 Sidebar，宿主装配不在本 issue slice 内。公开 API 独立 consumer 与完整 UI workspace 验证已通过；前者仅编译 API 与运行错误配置门禁，不证明双端行为。

新增 `ui/demo/tests/sidebar_multi.rs` 覆盖固定集合校验、两个隔离 endpoint 的同 session ID 双行、非首 endpoint selected read、aggregate owner 与 selection generation fencing、rename action 路由、row 移除后旧 action ticket 拒绝及旧 owner ticket 拒绝。`ui/bindings/src/sidebar_multi.rs` 的纯聚合单测覆盖首轮 terminal failure 的收敛语义、已加载端点故障后的 last rows/stale 保留，以及同 activity 下完整 address 稳定排序。子会话交付静态代码后，主会话已完成 2 项聚合单测和 3 项双端夹具测试，完整 UI 634 passed / 1 项原有忽略，严格 Clippy/格式通过。具体证据与未验收项见[本批收据](rust-agent-ui-parallel-results-20261024.md)。

Issue #22 的这一部分应以组件/绑定测试、AgentKit workspace 验证和公开 API 第二 consumer 验证为验收依据；本阶段不要求桌面验收，也不因未跑桌面而阻断这些无 UI 数据和路由契约。TekesUI #44 仍保持独立依赖关系；若其基础组件缺少实现所需事件或属性，应另行报告并提 issue，不在 AgentKit 里 fork TekesUI。
