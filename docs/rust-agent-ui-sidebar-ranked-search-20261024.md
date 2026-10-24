# Sidebar 全保留名称语料与查询排序设计（#22）

## 已确认的最终消费契约

当前 ClientKit `sessionSidebarSnapshot` 不是只在当前可见行上计算名称排名。`ClientMirrorSchema.swift:359-365,762-776` 的 FTS5 只索引 display_name，mirror_threads 的增改删无条件更新 FTS，因此当前镜像实际保留的 archive/child/ephemeral/其他端点名称也进入语料。它不代表所有 harness 历史都被读取。`ClientMirrorStore.swift:2686-2735` 先施加 main/archive/ephemeral/端点/目录/query 成员条件；普通 recentActivity 使用 bm25 升序、活动降序和 ID，provider-only 默认 1000000。标题排序不使用评分。

必须同时看最终 `ClientSessionSidebar.swift:27-46`：标题会再次使用系统 localizedCaseInsensitiveCompare，pins 最终仍按 pinOrder、身份平局排序。现有 Rust 宿主标题 comparator 与 pin_order 有直接依据，不能误把 SQL 的 pinned activity 顺序当作最终显示规则。

实施前 AgentKit FTS 仅从 SidebarPublication.rows 建索引；web inventory 已排除 archive，bindings 又排除明确 child/ephemeral，且单端投影只看到该端。直接加 bm25 字段无法得到参考语料，不属于完整对齐。此前交付的是名称成员并集；本轮实现见文末状态。

## 实施管线与所有权

1. 在 AgentKit 公共 SharedAgent 状态投影增加独立的 retained-name snapshot，同一次 MemoryProjectionStore snapshot 同时产生旧 inventory 与名称语料；不改变 sessions 的显示成员契约。名称文档只含完整地址和规范名称，不复制 transcript/history，不读取 harness 私库，不引入 SQLite 到 headless。规范标题提取迁移成纯共享函数，让行和语料共用显式 clear、legacy title/name 与 New conversation fallback。
2. 语料包含当前 canonical store 中实际保留且有有效 summary 的名称，不按 archive/main/ephemeral/folder/query/pin/page 过滤。真正删除或清除 summary 才移除文档。逐端保留 endpoint/connection epoch、原始版本与 inventoryReady；名称与 rows 必须来自同一个原始 frame。禁止投影时额外读取另一份 runtime 当前值来凑齐语料。
3. 单/多端 Sidebar publication 携带同代名称快照，多端从原始子 packet 一次合并全语料，再统一计算 SQLite bm25。语料域是固定 source 内的 endpoint 集合；正常 demo 使用 SharedAgent 中全部配置的 endpoint。当前 Run/selection、query、目录和分页不改变语料域。显示端点集合与语料域必须分开描述；严格排名路径不承认缺失/不完整的域，不能悄悄在可见子集上评分；某端首次观察失败时仍保留已知成员与原故障诊断，以既有就绪语义明确结果未完整，不能让评分待定遮住其他端已知结果。旧 membership convenience API 的兼容边界要明确记录，不冒充全局排名。
4. Shared SidebarSearchProjector 真实消费明确的 SidebarSort：recentActivity 普通行按本地 bm25 / provider-only 1000000，再活动降序，再稳定身份排序；标题模式交由原组件宿主 comparator；pins 继续由组件 pin_order 决定。评分是内部排序数据，不添加无人消费的公开 rank 占位字段，不在宿主新建排序状态机。
5. 排名在全局分页之前，分组/分页/selected 补读、query-source 与库存 revision 门禁复用既有实现。同 source/revision 的不同语料或来源元数据拒绝；替换 source 重新定域，连接恢复不复用混代数据。保留原 provider 失败/Retry 和 local/provider readiness 语义。

具体 DTO/API 需要由这一完整闭环倒推。新增只读字段不意味着排名已经完成；分阶段提交必须分别报告已接通部分与剩余消费路径。

## 验证与尚未证明的边界

- 同一份语言无关全语料夹具执行参考 SQLite 查询与 Rust；隐藏 archive/child/ephemeral/另一端点记录改变评分却不能进入显示行。
- 本地与 provider 同时命中去重；provider-only 默认值；活动平局和跨端同 ID；规范身份编码的 tie-break 需对照 ClientKit，不凭地址元组猜等价。
- 改名、显式 clear、保留归档、真正删除；query/folder/page 不改语料；source 替换、同 revision 不同语料、跨 epoch 混合拒绝。
- 标题 comparator 和 pin_order 不受评分影响；全局排序后分页，边界无重复/漏项；实际 demo 及独立 public consumer 消费新路径。
- 审计 MemoryProjectionStore 的保留/清除生命周期与 ClientMirror stale-thread 清除。算法对同一语料可核验，不得据此宣称两种存储实际保留集合、全 harness 历史、持久恢复完全相同。未从公开 SEP 获得的记录不伪造。
- root/UI 适用测试、严格 Clippy、边界、精确 SDK pin、正常 demo build 分层验证；桌面/物理 IME/AX 仍不运行。TekesUI #53 严格阅读门禁不变，不因本项交付而关闭 #22 或合并 PR #10。

本设计复用现有 #22，不重复建排名 Issue；独立 advisor 已核对语料与最终 pinned/title 消费，主会话保留架构与集成责任。

## 本轮实施与剩余边界

来源、Single/Multi binding、persistent projector 与 demo 已接通。显式 `*_with_sort` API 消费组件原有偏好；旧便利 API 保留成员投影/原输入顺序，不自动启用排名。完整域才应用全局排名；缺失端采用已知行名称作成员匹配，但不宣称完整排名或权威本地空态。名称与顶层 `connectionEpoch` 同 frame 校验，不在消费时读取另一份 runtime epoch；新 frame 缺名称语料时退为 legacy 来源，不把旧语料拼到新 rows 上。故障仍保留原 packet。

复核发现 ClientKit 搜索结果也受 `sessionLimit` 限制，已移除旧搜索无限展开规则；全局排名在组件页截断之前，搜索仍不补插当前 selection。普通 recent 排名平局使用 ClientKit `SessionEndpointStorageIdentity.thread` 的 UTF-8 长度前缀 key（含 legacy native 特例），不是按 endpoint 字符串元组猜测 SQL t.id 顺序；实际 Rust 地址和路由不变。既有 pins/title 最终身份平局仍沿用组件完整地址规则，不能据此宣称所有 Unicode/历史 durable-ID 排序完全一致。

生命周期只读审计：AgentKit inventory baseline/remove 清除 summary 但保留 session/history 容器（`crates/session/src/projection.rs:545,605`）；corpus 只包含仍有 summary 的名字。ClientKit replaceEndpointInventory 清理 endpoint 缺失 thread，stale cleanup 还会清理关联投影（`ClientMirrorStore.swift:800,873,3547`）。两种存储的清理时机、实际留存集合没有被证明相同；本轮验证相同输入语料上的算法及来源边界，不伪造未读历史。

只读参考 ClientKit HEAD `c289bc9d33edcd83a782ac17927a8c65a027eccd`，相关文件干净：MirrorStore SHA256 `506f9e6886f244859e43cfcbda57d683c3f0b4fb22ec2e9841851cb104abb761`；StoreContract `95155c7ac4ac6f8ab4d8dffc90b98952b0217233c24a8b6da8351a9f6ef8b812`；ClientSessionSidebar `c9a09d4dd40415e38ec08c5941fa729e981a0ed287692af0b836426f4e25457d`。未改 Swift。

完整执行证据见本批 ranked-search 结果收据；真实 harness/窗口/AX 与远端 cursor、精确保留集合仍未验收，不关闭 #22。
