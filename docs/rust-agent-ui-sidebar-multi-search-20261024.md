# Issue #22：多端搜索共享契约

目标是 ClientKit 的多 endpoint 搜索行为；业务状态属于 AgentKit presentation/bindings，宿主只输入 query、订阅、转发原始意图。固定端集合复用每端 SidebarSearchController，保持其 debounce、epoch、generation、能力和读取门禁，不用一个虚构 endpoint=all 的 SidebarSearchKey。

## 共享协调器

新增 SidebarMultiSearchController，内部独占固定端集合的单端reader；输入(runtime集、query)，输出独立owner/query generation/display revision与逐端原始 SidebarSearchPublication。相同query不重新查询；A→B→A有新aggregate generation，clear/drop退休旧资格，不调用session.cancel。端内重连继续由原reader重读；聚合采用当前reader快照，不能按相同query字符串接纳排队的旧publication。clear时全部child无key/hit，不允许旧结果恢复。

Retry intent必须携带原aggregate key和原child key，派发前同时验证，重试只作用原失败端；旧owner/query/child attempt不可重试新来源。未知/unsupported/失败保留逐端状态，不把局部失败变成全量空；raw hits、unresolved、hasMore保持逐端原语义，不声称全量结果。独立pins/本地Mirror FTS与当前ClientKit差异继续单列对齐，不能用聚合器悄改已有单端策略。

## 接线顺序与验收

1. 共享presentation协调器与双端可控协议fixture：相同sessionID、A→B→A、clear/drop、重连、单端失败/不支持、原失败端retry、旧票据和迟到响应。
2. bindings逐端原inventory join再按完整SessionAddress合并，原snippet/unresolved/局部错误保留；原动作用live多端ticket。
3. component + demo + 独立公开consumer消费新aggregate契约；不在宿主合并hit/造single publication。

远端搜索续页当前没有公开query/cursor契约（request只有query，results只有items+hasMore）；列表本地显示分页与远端续页分开验收，hasMore时明确提示缩小范围/完整性未知。不读extra.cursor猜协议，不重复query冒充下一页。ClientKit当前远程searchHostContent只读DSH，Mirror本地FTS另有来源；因此完成多端provider search仍不能宣称完整本地FTS parity。

主会话审查与集中构建，复用共享target。只消费精确公开TekesUI，不改SDK/Swift/Kernel，不运行桌面。

## bindings 接线的明确输入/输出

`SidebarMultiPublication::project_multi_search_for_display(search, display)` 接收真实 aggregate publication 与原 display revision/filter/selected，先按固定 endpoint status 拆出原单端 inventory，调用原 `project_search`，再按原全局 inventory order合并完整地址。输出独立 `SidebarMultiSearchProjection`，包含 SidebarInput、snippet、unresolved与逐端原状态/aggregate key；不能把多端伪造成一个 single `search_help.source`。partial/pending/error和hasMore继续独立可见，只有当前已返回且已join的结果可展示；没有凭snippet构造rows/actions。清query恢复全inventory，旧query/未知endpoint/重复child/source mismatch结构拒绝。

该层先提供可复用公开投影和定向验证，再由组件消费aggregate状态；投影通过不等于控件/宿主已接通。

## 第一阶段实施与已运行验证

`SidebarMultiSearchController::{new,new_with_messages,owner,snapshot,subscribe,set_query,retry}` 已实现，原子更新aggregate query生命周期、私有端内reader、通知只作唤醒并读取当前child snapshot。每端原始publication/key保留，不改其epoch或制造全局single key。输入相同query不重发，清空/Drop发布退休；重连只重读原端，retry同时校验aggregate和childkey。

7项真实SessionEndpoint协议fixture测试通过：同ID跨端保留、相同query一次读取、ABA用provider call identity确认旧A不能填新A、清空与迟到回包、Drop后不复活、单失败端retry/旧owner拒绝、unsupported端不抹其他结果、单端重连不重读另一端。完整presentation234 passed / 0 failed / 0 ignored，严格all-target Clippy通过。日志 `.runtime-fixtures/sidebar-host-20261024/presentation-with-multi-search-tests.log`、`presentation-multi-search-clippy-final.log`。没有假造成功publication替代reader，也没有运行真实harness/桌面。

bindings、组件和demo跨端query接线仍是后续阶段；第一阶段通过不代表完整Sidebar搜索验收或Issue关闭。
