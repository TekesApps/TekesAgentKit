# #29：真实 Sidebar 分类与历史保留的下一阶段契约

关联 [#29](https://github.com/TekesApps/TekesAgentKit/issues/29)、#22、#14。前置：[分类来源矩阵](session-endpoints.md#sidebar-分类事实29-2026-10-24)、[真实三端家族来源](rust-agent-ui-real-task-family-results-20261024.md)。只改 AgentKit，仍不启动桌面验收。

## 可证明事实

当前 SidebarRow 已压缩了原 canonical summary，不能从展示行反推 origin、ephemeral 或 parentSessionId。原成员谓词只排除明确 origin=subagent 或 ephemeral=true；缺失保持兼容显示，不能称已确认 main，parent-only 不能当 subagent。

Native0.9.3 普通 inventory 可省略 origin，fork 才明确 origin=fork，ephemeral genesis=true 时才报告 true；Pi 的规范化 summary 明确给 ephemeral，origin=subagent 来自 subagent_parent；DSH0.2.1-rc.2 可透传 origin/parent，但公开 SessionSummary 没有 ephemeral。child catalog 是独立 parent-bound 来源，不能把 kind=child 回写成 inventory origin。

## 小切片与所有权

1. 原 summary 字段的只读 DTO：逐字段区分 Missing、Known、Unknown（保留原 JSON）；显式 false 与 missing 不合并，null/类型错误/空身份文本不能变成 false/main。仅解析，不复制成员谓词或引入权限/IO/状态机。
2. 同帧来源携带：在原 SidebarInventory::read 处理原 item.summary 的同一轮捕获逐 address 原字段及原成员判断，经原 publication/host packet 透传。不能稍后另读 SharedAgent snapshot 再拼旧 revision。公开 API 若扩展，所有实际构造点、demo及独立consumer一并更新。
3. 真实样本与收据：按已固定公开能力生成/读取 ordinary、subagent、parent-only/permanent fork，以及 Native/Pi 支持的 ephemeral；DSH 不制造不存在的 ephemeral。归档仅记录实际 archive authority，未知与 inventory 中缺行分开。
4. 历史保留：每个排除样本在前后从同 endpoint/session 的原 lease/history API 比较 canonical scope、revision/throughSequence/items；仅保留名称或目录行不算历史保留。原 selected-row 补读必须沿同一成员谓词，不复活被隐藏行或 reveal。目录、分页、搜索与显示继续读取同一原 packet。

## 断言与报告

原始字段、同帧 source revision/owner、原 predicate 结果、实际展示 membership 分别记录；仅展示兼容性不提升为 main。先做字段拒绝和同帧 fixture 回归，再做公开真实三端；受能力限制的样本明确 unsupported/unknown，不能由 fixture 顶替。headless、组件、离屏、真实harness和桌面分层报告。

第一切片的 DTO 测试不构成真实分类验收。需要后续同帧携带、真实 producer 样本和历史保留链路全部满足后，才重审 #29 的关闭标准；不会借此降低 #53 或允许合并/发布。


## 真实生命周期的证据边界

归档回执、同帧库存成员和历史新鲜度分别记录。Native 0.9.3 不提供 workspace inventory；成功 `session.archive` 返回的 `archivedSessionIds` 是公开操作回执，不能伪造进随后 frame 的 `archiveFacts`。Native 的原 summary 被库存 remove 清空；DSH 可以保留 summary，同时由 ready 的 workspace archived IDs 排除。两者均不能只凭缺行推断归档。

归档会主动退休原 journal：`journal_ready=false` 不表示缓存内容消失。验收比较原 canonical transcript 的非空稳定项、scope、history revision 和 throughSequence，分别报告保留内容与新鲜度。恢复必须执行公开 unarchive、等待原库存恢复、reconnect、重新 open；归档态直接重新打开语义流不列为已支持能力。

离屏分类 oracle 只携带实际公开返回的原 address，供原 `read_selected` 断言已归档/隐藏项不能补回。oracle 不创建行、summary、session、pane、归档事实或调度权限；分类元数据依旧来自原 publication 同帧。

历史失败全部保留：Native Task 来源的默认 fork 曾返回 internal，尚未确认原因；普通来源 fork 已单独验证，不能据此宣称前者修复。旧脚本等待不存在的 Native workspace archive authority、要求 DSH 归档后必须清空 summary，以及通用 proof 单端 guard 拦截新三端 profile 的问题，分别作为验证脚本/入口修正，不当作 harness 修复。


Pi scratch 的存储与身份属于原 endpoint 实例：新连接器初始化会清理原 scratch。原实例的真实历史保留证据与退休事实分别记录；当前 demo 要验证 ephemeral 成员关系时，必须在当前原 runtime 上公开 fork 并持有原 RunBinding，不复用旧实例地址冒充跨实例存续。[本轮实际结果](rust-agent-ui-real-sidebar-classification-results-20261024.md)已完成该额外闭环。
