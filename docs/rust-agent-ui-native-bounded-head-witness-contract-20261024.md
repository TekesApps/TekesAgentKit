# #17 / #23：原 Native 有界 head 来源见证

基线 `9039d5f`。本轮在默认关闭的 `acceptance-source-witness` 内增加 built-in SameSession head-only 见证，复用原 ChildRun / bounded mutex、原公开读取、原投影及绑定 publication。独立/自定义 source 不支持，不能旁路查询或退回父历史。

Presentation 的 `ChildRun::transcript_with_bounded_source_witness` 返回同一次本地读取产生的 Transcript 与原 typed head 请求/响应、父scope、child及页大小。BoundedHistory只在feature内保存未经聚合改写的原head；原 `page.has_more_before` 可能受累计历史影响，不能冒充raw响应。普通读取和见证共用原refresh，持有同一mutex直到捕获，不再锁一次取另一帧。

**本切片不证明累计分页来源。** 成功前必须核对投影用的累计entries逐项等于原head.entries，effective has_more_before也等于原head；存在额外旧页或自然增量保留时明确拒绝见证，保留正常缓存，不为通过而清空、重查或编造。服务端typed响应不是原网络字节；refresh可能额外校验旧cut，不能称单RPC或服务端事务快照。

Bindings抽出共用snapshot构造路径，普通 `snapshot()` 行为保持；新 `snapshot_with_bounded_source_witness()` 返回原 `PaneSnapshot` 与该次读取来源。只分配一次publication，沿原source/scope、权限前后、epoch/connection与document检查；parent退休、投影或读取失败、epoch变化均不得返回成功见证。原controller RunSourceWitness保持独立，不能接收只读来源。

验证至少覆盖原请求数量不增加、head/累计差异拒绝而缓存保留、原error/stopped/source-call与投影、错误方言拒绝、原pane退休与同publication配对；同时验证默认/feature、实际宿主构建及独立公开API consumer。所有Cargo串行复用SSD共享target，保留源码/失败/程序/日志/固定输入，无新target，不改Kernel/SDK/兄弟库/锁，不跑桌面、物理IME或OS AX。

该接口步骤不声称已完成真实Native running→failed同pane验收，后续实际demo/owned Kernel闭环单列。实际分页、完整恢复/布局/IME/AX、完整ClientKit及#53仍开放，不合并/打tag/发布。
