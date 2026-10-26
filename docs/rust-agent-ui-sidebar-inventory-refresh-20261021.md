# Sidebar 未知搜索命中的公开库存恢复

对照 ClientKit `SessionEndpointRuntime.searchSessions` / `refreshInventory`：先完成公开搜索读取；命中未进入当前 session snapshot 的身份时，通过正常库存 baseline 刷新一次，再返回结果。Tekes 的 Sidebar host adapter 仍只提供产品导航/分组能力，宿主不各写一套搜索或库存恢复状态机。

## 共享运行时

`SessionRuntime::search_sessions_at_epoch(epoch, query, rpc_id)` 在 typed 解码之后检查规范库存。存在 summary 缺失的命中时，调用新的 `refresh_inventory_at_epoch(epoch)`；刷新失败就是搜索失败，不能把错误当空结果或把不受确认的命中变成可写会话。完成后再次核对连接/能力/epoch。没有未知命中不发起额外 baseline；一个搜索结果只主动刷新一次，仍缺失的命中继续保留为 unresolved，不循环扫描。

公开 `refresh_inventory_at_epoch` 在原 runtime actor 入场和提交时校验原连接 epoch，只重开 session inventory 以及 endpoint 明确支持的 workspace inventory。它不重连 endpoint、不获取新 journal、不关闭现有历史 lease，不调用 recover、prompt 或 Agent cancel。消费者观察同一个内存投影；无需 mirror 或私有数据库。

带 workspace-sync 的 endpoint 必须返回同 generation 的两份 baseline，不能先发布新 workspace 再等待 session baseline。全部 summary、projection、成员身份与 workspace 数据先校验；重复/空身份、坏 projection、错误首帧或 generation 不匹配时，规范状态保持原样。准备阶段只暂存库存数据，不复制已加载 journal/preview/history。提交保留已有历史、分页 cut、流式预览、执行错误与控制状态；缺失成员仅失去 inventory summary 权威，不删除可读历史。

每次成功提交使用独立 reader epoch，并撤销旧 inventory/workspace reader。旧 cursor 的晚到数据及其终止错误不能写入新 reader；新 cursor 的后续 delta 正常继续。connection epoch、journal reader 身份及 mutation 语义不变。

重叠请求共用一个 actor flight：读取期间新增请求标记后续 pass，多个等待方在最后一轮完成后一起收到结果；旧慢读取不能在更晚请求完成后倒灌。每轮读取有 30 秒超时，等待者有界；失败不自动重试，不丢弃仍可用的旧 reader。重连/关闭淘汰在途任务与等待方。

Query 离开会撤销其结果 reader，但已入场的共享规范库存刷新可以继续；它不是远端 Agent 执行。Controller 仍以原 query/owner/generation/epoch 拒绝旧结果。新 query 可以使用同一连接继续搜索，不能被旧 query 的 canonical 库存更新回退。

## 消费侧闭环

第二公开 consumer 创建 `SidebarSearchBinding`；搜索命中的新 session 只在公开 baseline 确认后进入现有 `SidebarBinding` 行和共享动作，再由原 `SidebarInput` 组合显示。UI 不从 snippet 或 provider 附加字段制造 `SidebarRow`，也不因匹配改变选择、草稿或提交。

owned native `--components --transcript-content --sidebar-query-selftest` 现在执行：失败 → 真实指针 Retry → 搜索返回尚无 inventory 通知的身份 → 原公开 baseline 刷新 → 规范库存行到达 → 清空 query。检查原行实例、原连接 epoch、原草稿和无 submission；刷新后的截图使用 `AGENT_SIDEBAR_QUERY_REFRESH_CAPTURE`。它仍是 fixture，不等于真实 harness 发生过未知会话竞态。

真实三端脚本的 `--ui-sidebar-query` 另显式调用公开库存刷新，核对连接 epoch、已加载历史/preview/分页 cursor 与搜索结果保持。这是 Native/Pi/DSH 真实协议和离屏 GPU 证据；不把预先已进入库存的真实命中说成未知命中竞态。精确 runtime、源码、日志、窗口及图像范围见 [同名 JSON](rust-agent-ui-sidebar-inventory-refresh-20261021.json)。

## 仍需完成

本轮补的是未知命中进入规范库存的闭环，**完整搜索结果列表仍未对齐**。ClientKit 的本地 FTS prefix 与 host ID 的 OR 成员关系、过滤后排序、snippet/help、搜索 header/count、多 endpoint 聚合及项目/文件夹 outline 仍需由 AgentKit 继续实现。规范库存多了一行并不等于最终搜索列表已经正确过滤。当前默认公开 TekesUI pin、545 和新收到的 292 开发候选均不因此被替换或提升。

物理 IME、VoiceOver、配对 ClientKit/Gold7 视觉及真实三端阅读/分页锚点仍需独立验收，不由本轮库存刷新证明。
