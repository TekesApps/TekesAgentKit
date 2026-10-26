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

本设计已完成只读来源审查；此前候选提出的三处来源规则已按上述条款明确。观察器已实施，两角色各8项Rust定向检查、严格Clippy及构建通过；尚未取得新cohort协议通过。仅重编相同observer的baseline/candidate两份现有example，复用SSD共享target、固定源码和程序；不改Rust库行为。先跑定向来源逻辑检查，再以新登记的Pi/DSH lifecycle补充cohort验证初始化+10轮，两角色各一次。

新cohort不能修改原40收据或把原失败改成通过；单列来源契约版本、observer/program SHA和原条件失配原因。原Native、Pi/DSH warm相对预算失败仍然有效，不调1.2门槛、不挑样本。仅重连收敛判据变为冻结ClientKit一致、来源更强的检查；性能绝对/相对/RSS/cleanup预算及完整#55/#56、UI/桌面/发布门禁均保留。

## baseline 适用接口的明确边界

第一次v1 baseline编译暴露观察器错误引用了f1尚不存在的queue常量；失败日志/源码保留，无协议执行。冻结f1的DSH adapter直接接收SDK inbox，102 adapter另派生agentkit.queue.v1。这不是把支持失败标Unsupported：本补充cohort是ordinary lifecycle，两角色均强制完整原生inbox及各自全部公开projection键和值；102另外强制derived queue键，f1不伪造后来的派生API。共同观察器由runner从已校验program receipt注入精确observerProductSource（仅f1/102白名单）选择该既有版本规则，不能按样本是否缺键自动放宽。真实queue/附件能力与性能仍在原矩阵，未由ordinary验收替代。[固定SDK来源hash](evidence-bindings-20261026/dsh-reconnect-source-contract.json)。

## 编译检查点（非协议验收）

[共同观察器程序绑定](evidence-bindings-20261026/reconnect-source-observer-programs.json)固定同版两文件overlay。baseline 10c630a4、candidate34c0707b只表示已编译/定向检查，不是生命周期或性能通过。新增source帧与每个stat bracket起止均在原事件流保留，缺来源先失败而非None相等；来源等待由公共subscription唤醒，整个follow/首帧有timeout，旧流在reconnect前释放。

v3候选首次编译复用了之后生成的baseline相对dep-info，错误引用旧字段；原因是两角色提前一起准备mtime。已保留失败日志，源内容hash不变，仅在候选自身串行批次前刷新时间戳后通过。后续每个角色在进入自身批次前准备时间戳，不在另一个角色构建前预先准备。所有构建仍复用唯一SSD target，没有创建临时target。

## v1实际执行后的契约纠正（拟v2，仅DSH）

四case原结果见[补充绑定](evidence-bindings-20261026/reconnect-source-official-cohort.json)。Pi两角色11轮来源真实通过（仍有一个分页相对延迟失败），不重跑。DSH两角色均在首个测量轮scope0 before fresh cut469/196条记录之后、scope1 before之前超时，尚未执行该测量轮reconnect；未保存predicate细分，不能判产品缺陷。

独立只读审查确认上述“断线前runtime所有值追齐首份fresh witness”是本观察器新加的过强前置，不在冻结ClientKit重连契约中：c289 SessionEndpointRuntime.swift:456–482取消旧任务并重新start；:1160–1191建立新的journal首帧恢复；:3332–3379独立启动control流，不在断线前逐项追平。因此拟v2只撤除这个额外前置，保留持久来源与恢复后标准：

- before抓首份独立fresh follow，保留完整源帧；统一冻结并记录旧runtime的每个键/value/seq与完整history。旧seq高于fresh cut必须失败；缺失或滞后明示证据，不要求先追平，不能假装before已同步。
- 两独立fresh before/after的cut、完整值/entries/窗口必须相等；重连后的完整runtime projections、旧seq不回退、完整entries及summary/time来源仍必须在原5s内达到after fresh来源。原pointer/成员/epoch/new generation/无重复与历史ID/内容/顺序标准不变。
- 每次after等待无论成功/超时均记录原runtime完整快照与predicate细分，避免无法定位失败；不根据缺字段自动放宽。
- 只为这两个受变更影响的DSH角色登记新case各执行一次；原四case、原40case与有效性能失败原样保留，不混算新旧观察器、不重跑Pi/Native/warm、不改预算。代码与验证尚未完成，不据本设计称DSH通过。

### 公开来源窗口必须覆盖已保留窗口

进一步用原v1事件离线核对，两角色init fresh50均196条且hasMoreBefore=true；measured round1公开close/open记录缓存前后均204条，但fresh50只196条。因此“runtime整个已保留窗口==rolling50来源窗口”本身也是错误前提，至少history predicate无法相等，不是产品丢失证明。v4构建输入保留，不对其登记或执行协议。

v2采用独立公开follow200覆盖本有限case的全部消息（100正文+最多22追加+10工具组，非200条raw records），强制hasMoreBefore=false，原始完整源帧全部保留。before runtime的原最小seq作为retained floor，必须真实存在完整来源；从完整源按seq>=floor取精确后缀，逐条比对完整entry内容/顺序。after complete source仍须与before全部来源相等，after runtime必须恢复同一个floor后的完整后缀，不能丢旧行或只比ID；不能强制加载用户此前尚未加载的更早前缀。SDK的恢复窗口仍50，原Earlier20指标不变；200仅为独立验证来源，观测开销计入reconnect且两角色相同，不混算v1时间。若200仍未耗尽来源则失败，不自动扩容/放宽。

### v2独立审查与构建

独立静态review未发现放宽原恢复标准，确认完整来源与原retained floor的逐条比较、after全部投影/水位/summary及5s有界恢复。精确v5输入两角色各9项定向Rust检查、strict Clippy及构建通过，程序baseline950db9a1/candidate92206559，详见[新程序绑定](evidence-bindings-20261026/reconnect-source-observer-v2-programs.json)。源内容在每角色串行批次前后均核对不变，mtime在各自批次前准备，复用SSD共享target。两条DSH新case尚未登记/执行，不称通过；Pi/Native不重跑，全部旧失败和旧样本保留。
