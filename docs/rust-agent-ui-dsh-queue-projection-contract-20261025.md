# DSH inbox 的通用队列消费契约（2026-10-25）

归属 #16/#27，关联 #15/#21/#22/#28，PR #10。真实busy inbox已有协议证据，但RunPresentation.queue、Indicators与Composer仍未消费；当前ClientKit相同遗漏不构成完成依据。只修改Rust AgentKit，不修改Kernel/TekesUI/ClientKit，不读取私有history/DB，不引入第二套inbox reducer。

DSH adapter独占语义转换，保留原inbox wire与原sequence：next-turn按内部顺序映射原message.id、placement=queued、target=next-turn；next-step明确placement=next-step、target=next-step，保留组内顺序，不宣称全局FIFO或已执行。原message完整保留；source.rpcId仅确认原user提交，不能取代item.id。context/tool来源不成为用户编辑或回显确认权限。

只接受固定公开inbox的两数组结构、非空唯一message.id、完整message对象与合法source；缺失inbox不合成权威空，明确两数组空才清空。畸形/重复身份/迁移hint不转换；不从agent/inbox/spliced自行重放。转换只在已确认DSH adapter入口进行，不解释其他endpoint任意同名opaque key。

内部标准化projection key为 `agentkit.queue.v1`，value精确 `{items:[...]}`，使用同一原asOfSeq/seq。DSH所有原始入口拒绝远端自带保留key（baseline、control delta、journal opening/delta、inventory、history）；adapter随后原子附加该key，保留原inbox。新默认None的SessionEndpoint.sequenced_queue_projection_key()显式声明本地adapter的key，HostedEndpoint透传；Native/Pi不启用。Runtime只从真实endpoint配置MemoryProjectionStore，不能根据远端key或任意inbox自行授权。

generic projector只负责已授权标准化key的版本接受和queue物化，不理解DSH next-turn/next-step。所有inventory/journal/control入口进入同一SessionProjection版本门禁；接受后与原projection一起物化queue。同seq同value幂等，同seq不同items拒绝；旧seq忽略，旧generation/epoch按既有机制拒绝。control baseline不得先清空由更高版本journal产生的queue；缺失/旧的标准化key也不得冒充清空。仍保留Native/Pi原无序号queue行为，不以其覆盖已授权有序号队列。

验证必须覆盖DSH公开shape→adapter→projector→Run/Composer，原RPC退休echo、原item.id队列动作身份、空claim/delete清空、seq20 journal与seq19 control双向交错/迟到baseline、同seq冲突及幂等、畸形/重复/source错型、remote保留key碰撞、非DSH同名inbox不确认、旧epoch、不误授予context权限。共享包/demo/独立consumer适用验证并保留not_run层级。

真实busy probe在原queueWitness之外新增commonQueue与submission字段（原RunSnapshot.queue/SubmissionState），独立runner核对它们与原inbox message完全一致，reconciled=true且同RPC无provisional。保留原scope/RPC/sequence/epoch、附件SHA和重连门禁；不得只证明inbox存在。Native实际busy control为空另留上游缺口，不以DSH通过代表Native。所有Cargo由主会话串行，复用SSD共享target；桌面不跑。

最终 busy 收据还要求 `drained` / `recoveredDrained`：原 scope/epoch、通用队列为空与原公开 control/inbox 明确为空；DSH 清空投影的原序号必须晚于排队见证。独立 runner 在完成和重连后分别检查，不能只用最终 assistant 代替队列移除证据。
