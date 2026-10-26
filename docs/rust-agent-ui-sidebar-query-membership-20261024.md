# Sidebar 查询成员关系修正（#22）

## 对齐基准

以当前 ClientKit 会话 Sidebar 的实际消费路径为准：`ClientAgentSidebarModel.reloadFromClientMirror` 调用 `ClientMirrorStore.sessionSidebarSnapshot`。后者将名称 FTS 与 provider 候选 OR 合并，先对共同 predicate 加 query/folder 条件，再划分 pinned、普通行及 selected。置顶是展示分区，不是查询/目录筛选的例外。

此前 AgentKit 将另一条 `sidebarSnapshot/globalRows` 的独立 pins 规则用到会话 Sidebar，属于契约解释偏差。历史测试/截图保留，但不再作为当前目标的行为标准。参考文件：ClientKit `Sources/Composer/Agent/ClientAgentSidebarModel.swift:408`，`Sources/Session/Persistence/ClientMirrorStore.swift:2684`、`:2703`、`:2725`。

## 第一切片：provider 成员关系与共同目录筛选

- 在共享 `SidebarPublication::project_search` 中移除 pinned 的无条件放行；固定多端投影沿用同一实现。provider 匹配 pin 可见且仅一次，未匹配 pin 隐藏，清空 query 恢复原完整库存。
- folder predicate 先约束全部行，再划分 pinned 与普通行；不因为 pinned 而绕过目录条件。
- 保留库存 loaded、原始 pin 偏好、选择和 Composer 草稿；隐藏不等于撤销底层会话或清除偏好。旧可见行控件不能继续执行已退休动作。
- 查询 pending/failed/unknown hit 继续显示其原有诊断，不展示未过滤的原库存为新查询命中；canonical 库存仍保留。空的待定展示集合不代表权威无结果，不将 provider unsupported 或失败标为查询完成。
- 原生 pin-query selftest 的“未匹配 pin 始终保留”断言和收据字段应改为新规范，但本轮只编译/静态检查，不启动桌面、不复用旧截图验收新语义。

验证必须覆盖单端、多端同 session ID、匹配 pin 去重、清空、无结果、provider pending/failed/unresolved、目录切换、偏好/选中/草稿保留与旧动作拒绝；宿主与独立 consumer 一并检查。第一切片仍缺本地名称候选，不能称完整 ClientKit 查询对齐或关闭 #22。

## 后续名称搜索：实际派生索引

AgentKit 自己拥有 Rust 的可重建内存派生索引，输入仅为原公开 inventory 的完整 SessionAddress、canonical title 和来源/库存版本。不访问 harness 私库，不加 Swift，不要求本地持久 Mirror，也不建立第二 canonical store。

目标使用真实 SQLite FTS5 `unicode61 remove_diacritics 2` 与 SearchFrame 的按空白拆分、引号转义、token-prefix AND 表达式，避免把 contains/starts_with 近似当作对齐。尚未引入 SQLite 依赖；须先确认精确依赖及跨平台 FTS5 构建能力。索引需要实现而非仅增加注入接口后宣称完成。

后续成员关系为 localNameMatches ∪ providerMatches，再与原 inventory 按完整地址 join。provider unsupported/failed 不撤销本地命中，状态及 Retry 保留；需拆分本地成员可用性与 provider 进度/unresolved，不能用当前单一 results_ready 阻塞全部本地结果。沿用 query owner/epoch/generation，增加原来源/库存 revision 的派生凭据；改名、删除、同端替换来源、observer 重启和 ABA 不复用旧结果。

名称索引最少验证重音、前缀、AND、引号/OR 字面量、中英文混合、改名/删行、跨端同 ID、unsupported provider、去重及共同过滤。ClientKit recentActivity 的 FTS bm25 排名、远端 cursor 和完整窗口验收另列开放，不把预留字段当作实现。

本设计经独立 scout 的当前源码对照及 advisor 的只读架构审查；未改 ClientKit、Tekes、TekesUI 或 Kernel。

## 后续依赖只读预审

尚未修改 Cargo 依赖。下一切片候选精确 `rusqlite = "=0.40.2"`，拟关闭 default features，仅启用 `bundled`；是否采用仍须通过实际构建和 FTS5/查询语义测试。该 tag 的 [Cargo.toml](https://github.com/rusqlite/rusqlite/blob/v0.40.2/Cargo.toml) 将 bundled 转给 libsqlite3-sys 并启用 modern_sqlite；其 [bundled 构建源码](https://github.com/rusqlite/rusqlite/blob/v0.40.2/libsqlite3-sys/build.rs#L138-L159) 明确启用 FTS5。这里只证明上游源声明，不代表当前 AgentKit 已编译/启用 FTS5，也不代表所有部署目标已经通过。

## 2026-10-24 第二切片实施契约

现采用精确 `rusqlite = "=0.40.2"`（关闭默认 features，仅 `bundled`），依赖只进入 UI workspace 的锁文件，根 workspace 继续不依赖 SQLite。实际 SQLite FTS5 的七项索引回归已经执行通过；不再把上面的“尚未引入”历史阶段描述当作当前依赖状态。

UI bindings 的 `SidebarNameIndex` 以完整库存地址/名称构建可再生内存索引。bindings 的 `SidebarSearchProjector` 持有索引，demo 复用它并传入原 observer source key、库存 revision；多端一次索引完整聚合库存，再按完整地址划分各端结果。相同 source/revision 的不同名称和逆序 revision 被拒绝，新 source 重新建立版本序列。公开 convenience projection 保持兼容，使用短生命周期索引；宿主接线使用持久 projector。source key 只标记派生缓存的来源，不认证任意调用方提供的库存。

当前行集合为本地名称 FTS 命中与已知 Complete provider 命中的并集；单个未知 provider hit 不再遮蔽其他已知行。provider 正在查询、失败或不支持时，本地命中仍可用，原诊断及 Retry 保留。`local_results_ready` 与 provider `results_ready` 分开：只有库存已恢复且 observer 正在观察，才确认本地空集；只有 provider 完成、所有结果可 join 且 has_more=false，才能显示完整无结果。其余就绪本地空集只声明“没有匹配的本地会话名称”。多端使用本次库存重新 join 后的 typed help，不从旧 publication.unresolved 推导当前就绪状态。

canonical title 的显式 null/空白清除不再回退到旧 summary；未命名会话使用 `New conversation`，路由 session ID 不会作为隐含名称索引。合法的显式名称即使恰好等于 ID 仍保留。

仍未完成：recentActivity/bm25 展示排名、远端 continuation/cursor、所有 harness 的 main/ephemeral 分类公开事实，以及真实原生/物理 IME/AX/完整像素 reveal。此切片不改这些边界，不关闭 #22，不绕过 TekesUI 严格详情阅读门禁。

落点经本轮独立 advisor 复核：原 headless 文档与依赖门禁明确禁止 presentation 依赖 SQLite，因此索引及其七项测试移至 ui/bindings，不改变门禁、不使用 optional feature 绕过，也不新增反向 UI 依赖。先前未提交落点的运行日志保留；最终证据以迁移后的 suite 为准。
