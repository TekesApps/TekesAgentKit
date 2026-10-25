# 重连收敛的来源契约（#55/#56，候选验证设计）

范围仍冻结5b及已批准阻断修复；ClientKit参照c289bc9，运行时Kernel0.9.3/Pi1.3.0/DSH0.2.1-rc.2。本设计只纠正验收观察器，不修改产品/SDK、不调整性能预算。原42计划、40执行收据和失败原样保留，未据此宣称生命周期已通过。

## 已确认的分层

ClientKit将inventory summary与live control独立消费，再在Mirror规范化读模型按水位合并（SessionEndpointRuntime.swift:821–879、3452–3479；ClientMirrorStore.swift:667–720、1278–1306）。AgentKit同样保留原inventory summary与live projections，Sidebar优先用live title和派生activity；CLI `list`只是runtime.inspect缓存，不是fresh SDK请求。

DSH单次公开补证：重连前后204条完整journal内容/顺序相等，scope0所有live value相等；scope1在测量后立即抓取的live turnOutline仍落后（seq466、response为空），恢复到cut469后补上完整response，其余值相等；不能据此宣称两个scope的live整值都相等。原summary为旧cut461，恢复后summary为469，scope0恢复后各值恰等于原live值，scope1的turnOutline需要独立fresh来源证明；两scope的updatedAt恰等于live sessionListMetadata.lastPromptAt。故整raw summary相等超出冻结参照，不宜为过此检查改产品。

Pi单次公开补证：123条完整native entries及顺序相等，summary仅updatedAt改变。AgentKit该字段在5b已从原session文件mtime派生，与ClientKit stored timestamp差异由后续#70跟踪。时间变化不能简单忽略或只验证递增，必须由实际公开定位文件的metadata证实。

## 拟验证的完整条件

所有端仍须新endpoint generation/epoch、原两pointer/成员关系、可用原来源快照、无重复历史、原ID顺序和完整entry内容一致；Native保持原summary比较标准和已有正式结果，不重跑或覆盖其真实性能失败。

DSH还须：

- 独立来源必须是同一公开 endpoint 新建的 `SessionJournal` follow 首帧，记录 sessionId、generation、throughSequence、完整 projections 与 entries。现有适配层负责 inbox→agentkit.queue.v1；验收不得另写归一化器。CLI list、inventory hints 和最终缓存 map 不能自己证明自己的 authority。
- 精确 SDK 依据：`@deepseek-ai/dsh-api-session-controller/lib/types/types.d.ts:66–70` 定义 baseline “at an exact Session event cursor”，`:538–546` 把 cursor 与该 baseline 放在同一 follow snapshot；`lib/types/history.js:196–218` 同一次 sourceFor 取 source.cursor 与 source.projections。因此 fresh follow 必须满足 `projections.asOfSeq == throughSequence`。这不是由旧样本469推导的通用假设。控制 baseline 可以独立复核值，不能单独证明 journal cursor。
- 在重连前取一份 fresh witness，确认 typed sequenced block 存在、全部键（含队列派生键）齐全。旧 runtime live 可能尚落后，必须在有界时间内收敛到该 witness 的完整值后再冻结 before；记录等待耗时，不能把“正在追上”改判为已丢数据，也不能无限等候。首份 witness 与来源水位不一致、失败、超时均保留失败，不挑选另一个匹配快照。
- 重连后新 endpoint/generation/epoch必须成立；新建 fresh follow，不复用旧订阅；其完整 entries、值和 cut 须与前 witness 一致，证明稳定来源无变化。每项 runtime live value 都须等于 fresh witness；sequence不倒退、不超过来源cut。所有 witness 流在退出 helper 前释放，不能延长旧 SDK 生命周期。
- 恢复后 summary 除 projections/updatedAt 外的稳定字段相等；summary projections 必须与独立 recovered witness 的完整 block 一致，不能忽略动态字段或只比最大sequence；updatedAt精确等于该 witness 的 sessionListMetadata.lastPromptAt，来源缺失即失败。
- 原 runtime before entries 与恢复后 entries 完整内容/ID/顺序一致，无重复；历史 window/cursor保持原标准。公开读模型继续采用现有 title/activity 来源，不向 Sidebar 塞未公开 turnCount，也不把 #70 排序语义拉入本期。

Pi还须：

- 原summary除updatedAt外全等；完整native entries、公开native sessionId与sessionFile定位保持相同，代际字段另按重连规则验证。
- 只对公开state.sessionFile定位、且canonical path处于显式benchmark-owned case根下的普通文件取metadata；拒绝越界/重定向路径，不读取会话内容或私有数据库。
- before与after都取metadata；每次围绕来源采集前后核对canonical path、device/inode、size和mtime sec/ns均稳定。采集中变化即失败/证据不足，不能挑一次匹配stat或静默重试。
- 恢复后的updatedAt精确匹配该次稳定stat的mtime，使用endpoint同一 `duration_since(UNIX_EPOCH).as_secs_f64()*1000.0` 转换；不要求旧缓存updatedAt等于pre-stat，不允许只检查时间递增。不能证明来源时不标Unsupported。

## 执行与门禁

本设计已完成只读来源审查；此前候选提出的三处来源规则已按上述条款明确。尚未实施、编译或取得新cohort通过。仅重编相同observer的baseline/candidate两份现有example，复用SSD共享target、固定源码和程序；不改Rust库行为。先跑定向来源逻辑检查，再以新登记的Pi/DSH lifecycle补充cohort验证初始化+10轮，两角色各一次。

新cohort不能修改原40收据或把原失败改成通过；单列来源契约版本、observer/program SHA和原条件失配原因。原Native、Pi/DSH warm相对预算失败仍然有效，不调1.2门槛、不挑样本。仅重连收敛判据变为冻结ClientKit一致、来源更强的检查；性能绝对/相对/RSS/cleanup预算及完整#55/#56、UI/桌面/发布门禁均保留。
