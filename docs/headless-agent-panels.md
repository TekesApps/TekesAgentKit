# Tasks、Subagents、SideChat、Summary（无 UI）

本层位于 `tekes-agent-presentation`，继续沿用 `SessionRuntime` 和内存投影。
不依赖 TekesUI，不增加 Swift，不引入持久化 Mirror。Native 子历史使用经授权新增的 Kernel 只读接口。
参考当前 ClientKit 源码及 Tekes 的 Summary/SideChat host adapter；来源校验见
`panel-source-baseline.json`。这些 API 提供状态、操作和宿主输入；不创建窗口或 tab。

## 任务与 session pointer

`AgentTask` 保存 task ID、所属 endpoint/thread、当前 session pointer 和 producer name。
`TaskPanelState::begin/commit/cancel` 用带 generation 的 token 隔离异步查询；同一个 task
可以映射到新的 session，缺失行会清空旧 pointer，失败查询不冒充删除。跨 endpoint/thread
回包被拒绝。root-local todo 没有独立 Run 导航；`task` 工具只取 `task_name`，不从结果的
brief/verify 或 lineage 推测任务名和子会话地址。

`SummaryFacts::read` 从公开投影 `todos` 和更晚的 `todo/write` 事件恢复 todo。
projection cut 优先于更早的历史页，null/空数组可清空成员。普通 tool call 不证明独立
session 的存在；宿主若有权威 task pointer 查询，可通过 `TaskPanelState` 和 `SummaryFacts`
输入规范化记录。这里没有把 fork provenance、team/task 或 job 伪装成 task 子会话。

## 子代理与历史

`SubagentCatalog` / `SubagentEntry` 与 ClientKit 的 entries / parentAvailable 契约对齐。
`ChildCatalogState` 在每次选择或连接代际变化时 begin，重连成功后重新查询再 commit。
loading、continuable、one-shot、parent unavailable、missing、read failure 分开处理。
宿主必须在断线时调用 `begin(parent, false)`；缓存 catalog 不能授权写操作。

`SessionRuntime::prompt_child/cancel_child` 每次控制前重读 catalog，验证 kind、mode 和
父子归属；文件续写明确拒绝；从不回退到 session.prompt/cancel(parent)。RunController
的非主 Run 提交/取消也走父拥有的 child route。直接使用 endpoint 原始 call 的调用者
仍须遵守其公开方法契约。

`ChildRun::open` 区分 SameSession 与 IndependentSessions：独立子会话持有自己的 pointer；
同 session 有界子线路默认使用公开 `subagents.history`，也可显式提供 `ChildHistorySource`。
服务端未提供方法时明确 Unsupported，绝不展示父 transcript 来冒充子线路。
`SessionHistoryLease` 按 mounted consumer 持有；Drop/release 只释放自身，保留其他 lease
以及普通 open_session 的 owner。没有 active Tokio executor 时由 runtime shutdown 回收。RunController 的 child Run 只有在新 catalog 验证后才允许输入；重连后调用 refresh_child_catalog，连接 epoch 防止 watch 合并通知后复用旧权限。

能力与实际实现分开：

| endpoint | 子代理目录/控制 | 子历史 | SideChat |
|---|---|---|---|
| Native Kernel 0.9 + 子历史扩展 | 原生目录；有界 child 不可续写/单独取消 | 内置只读子历史、冻结分页、source-call、重连 | ephemeral fork/discard |
| Pi 1.3.0 | create/list/prompt/read/cancel；官方扩展模型工具 | 独立原生 session pointer、恢复、重连 | ephemeral fork/discard |
| DSH 0.2.1-rc.2 | 从公开 subagentCatalog projection 规范化；continuable 控制 | 独立 session | 明确 Unsupported |

DSH 当前发布版已移除旧 `subagents/list` remote。adapter 通过 `session/projections`
和 `session/list.agentAvailable` 读取目录及父可用性；续写把 queue/steer 转为 delivery，
mode 固定为 continuable，保留 requestId，并把 messageId 规范化为 admission receipt。历史 follow/page 使用 parent/child/mode 地址，重连时从公开 inventory/catalog 恢复，不能把子会话作为普通 session 打开。
失败和迁移未完成的 projection 不产生空目录成功结果。不修改外部 ClientKit 或 DSH。

## SideChat 生命周期

```rust,ignore
let side = SideChatController::new(runtime.clone());
let chat = side.create(parent_scope, Some("Parent title"), "fork-rpc-id").await?;
// chat.parent 始终是发起者；宿主比较当前选择后决定是否显示 tab。
let run = RunController::new(runtime.clone())?;
run.open(chat.scope.clone()).await?;
side.close(&chat.scope.session_id, "discard-rpc-id").await?;
// 只有 discard 成功/权威查询确认 absent 后，才清草稿和 annotations。
run.forget_destination(&ComposerDestination::run(chat.scope));
```

同一 parent 的并发创建会被拒绝。必须支持临时 fork 与 discard 才开放入口；不降级成
普通持久化 fork。fork 成功先记录 pointer，再进行 best-effort rename；切换父页面不会
让返回结果归到另一个父会话。关闭时先获取历史 lease，取消正在运行的 turn，等待结算
最多 5 秒后 discard；失败保留 pointer。turn/end 与 worker 退出之间，服务端明确以 session-running 拒绝 discard 的情况最多等待 5 秒；只有这种确认未执行的拒绝才重试。超时、断线和 future cancellation 标记 Unknown，
不会自动重试。权威查询成功后用 reconcile / reconcile_creation 显式解除 unknown；查询
失败不是 absent，Closing 期间不能由 reconcile 覆盖在途操作。

`forget_destination` 同时退休 echo 和已捕获草稿，阻止较晚的失败回调把已丢弃会话的输入
重新恢复。宿主 annotations/tab 的清理仍由宿主完成。

## Summary

`RunSnapshot.summary` 自动读取当前 session 的可用公开事实；可选 Summary 数据损坏只
增加 transcript diagnostics，不阻塞正常 Run。`SummaryState` 为宿主额外的异步权威查询
提供 scope/generation fence，查询失败清空其结果；`SummaryFacts::sections(current)` 只
返回完全匹配的 scope。主 Run 与子 Run 不能混用结果。

默认顺序与当前 ClientKit 一致：tasks → subagents → sources → artifacts → usage。
空 section 消失；sources 去空白、去重和排序；artifacts 使用输入的 newest-first 顺序，
按路径保留第一条。导航输出为 typed action，由宿主执行；Git 等产品 section 经
append_summary_sections 显式追加。tool subagent 标题取 brief/description，启动结果的
首个 UUID 保留为 native ID，endpoint 不编码进 ID。

Usage 读取权威 tokenUsage，输入分母包含 uncached + cached + cache creation；分母为零
返回未知 hit rate。未知或不完整费用是 null；只在 isCostComplete 且费用有限、非负时
返回金额。不会用 token 数猜价格，也不把 per-turn usage 与权威 aggregate 重复相加。
Artifact 列表、带独立 session 的 task 和计价明细通过显式 SummaryFacts 输入；尚未为
缺失能力的 harness 新造任务调度、Artifact 服务或子代理引擎。

## 验证范围

Rust 回归覆盖 task pointer 更新、A→B→A、scope 隔离、catalog authority、断线/失败、
todo 历史 cut、usage、provider 顺序、draft retirement、lease 多消费者、SideChat 不确定
创建和失败丢弃、bounded history 拒绝 parent fallback。CLI 是独立公共 API consumer，
提供 childCatalog/childPrompt/childCancel、sideChatCreate/sideChatClose/sideChats 和含
summary 的 presentation。真实 runtime 验证使用临时数据和本地合成 provider；不代表
生产模型、UI/native 体验或所有 harness 的 task/subagent 能力已实现。

v0.2.0 基线验证：workspace 72 项 Rust 测试、strict Clippy、fmt 和依赖边界通过。真实隔离
集成覆盖 Native Kernel 0.9.0、Pi 1.3.0 的 SideChat submit→return→discard，以及
DSH 0.2.1-rc.2 / 0.2.0-rc.2 的实际 continuable child catalog→prompt→history→reconnect→cancel。
DSH SideChat Unsupported 单独断言。不是付费模型或 UI 验收。

后续 Pi 子代理和 Native 有界子历史的 API、能力边界与验证见 [child-history](child-history.md)。
