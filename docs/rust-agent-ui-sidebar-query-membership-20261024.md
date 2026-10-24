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
