# 已知性能失败：一次有界离线归因（#55/#56）

2026-10-26，仅检查原正式收据、事件、冻结 f1/102 源码及已有 Native wire 诊断；没有重采、变更预算或替换样本。来源与程序身份以 [ordinary 绑定](evidence-bindings-20261026/ordinary-official-cohort.json)、[Pi lifecycle 绑定](evidence-bindings-20261026/reconnect-source-official-cohort.json)、[DSH lifecycle v2 绑定](evidence-bindings-20261026/reconnect-source-official-cohort-v2.json) 为准。本次不产生新的通过结论。

| 失败门禁 | 原证据可说明 | 有界结论与缺口 |
| --- | --- | --- |
| Native lifecycle snapshot 1.661691 倍 | fresh open 到可用快照总耗时 | 没有 endpoint open、读取、解析及投影分段，不能指认库内缺陷。 |
| Native lifecycle assistant 1.360828 倍 | p95 candidate 368.01ms / baseline 270.43ms；dispatch、durable user 观察时间已增加 | 差异不限于助手内容观察末段；缺与同批 case/scope 关联的 provider/Kernel 阶段时间。旧 wire 诊断不属于这些样本，不能移作因果证据。 |
| Pi warm snapshot 1.341904 / 1.313128 倍 | 受测 open→usable publication 的总时间 | 其他初始化 snapshot 不是同一门禁操作；缺 RPC/follow、解析和投影分段。不能拿不同事件抵消失败。 |
| Pi lifecycle Earlier20 1.265616 倍 | 来源、内容、mtime、恢复均核对；整体分页耗时有效 | 缺公开读取与投影应用的分段，无法归到具体本库实现。 |
| DSH warm assistant 1.262329 倍 | dispatch、durable user、assistant 观察时间均增加 | provider 记录缺逐请求时钟与 scope 对账，无法确认差异原因。 |
| DSH lifecycle cleanup 1.273253 倍 | 原 OS 保守退休上界超相对预算，10s绝对预算通过 | 不等于产品请求延迟，也不证明本库可修；不得删除相对失败或把它改成成功。 |

**决定**：保留全部有效失败；当前证据不足，不做猜测性产品改动。Native Family先完成一个diagnostic端到端pilot，正式矩阵尚不启动。若后续必须补归因，只增加与原失败操作绑定的阶段时序（含case/scope、公开调用、provider响应及投影边界），先验证工具，再仅采受影响部分。新观察程序保留独立身份，不与旧observer混算；已经通过且未受影响的组不重跑。

这不是性能豁免或阶段交付。当前完整条件见[阶段唯一入口](agentkit-stage-close-20261025.md#当前状态唯一入口2026-10-26)。
