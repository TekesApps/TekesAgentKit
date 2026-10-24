# Native / DSH 真实繁忙队列契约（2026-10-25）

归属 #16/#27，关联 #17/#28、PR #10。承接3bda2f6的空闲mixed闭环；不把mode=queue等同真正排队。继续复用原RunBinding/presentation、固定公开runtime及TekesUI，不改上游、不跑桌面/付费模型/私有history。

在原mixed probe增加显式endpoint config `busyQueue:true`；缺省路径的receipt/行为保持。仅第一个有正文场景先发送 `NATIVE_DSH_BUSY_HOLD` 并由独立loopback provider hold。必须原committed非provisional held user绑定RPC及原运行状态，再提交原mixed ticket。第二个无正文场景保留原基线回归。

Native公开control.queue的item为id/placement/message，RPC在message.source.rpcId，content是公开物化block映射；DSH的真实事实位于公开projections.inbox.value["next-turn"]，不是独立control.queue。dsh-agent-loop注册inbox wire，sessionProjections.snapshot按已注册wire导出，API control baseline与onChanged转发该projections；不能因未独立发送queue frame就断言inbox不公开。source.rpcId从requestId原样传递。固定公开源码允许清单保留在对应SSD evidence public-source/manifest.json，禁止递归搜其他fixture。

只有收到与原scope/RPC、canonical内容、真实上传回执/图片hash对应的原队列事实，probe才输出单条 `AGENTKIT_NATIVE_DSH_BUSY_QUEUE_READY {json}` 并flush。runner独立确认hold请求已进入、业务queued请求尚未到provider、队列身份/内容/连接epoch/revision有效后才释放hold。禁止基于超时、模式或本地draft宣告queued；无公开来源则fail closed。

busy模式只在顶层增加busyQueue对象，原roundTrips与reconnect结构保持。busyQueue含holdRpcId/holdInput/holdAssistant/queueReady/recoveredHoldInput/recoveredHoldAssistant/drained/recoveredDrained；queueReady固定为status、endpointKind、scope、rpcId、ticketId、snapshotRevision、connectionEpoch、queueFact、queueWitness、prepared、uploadReceipt、imageInputs、holdRpcId、holdInput、commonQueue、submission。commonQueue 与原标准化队列一致，submission 必须为原 scope/RPC 且 reconciled=true。drained/recoveredDrained 分别记录完成和重连后的原 scope/epoch、commonQueue=[] 以及原公开明确空投影；DSH 清空序号必须晚于排队见证。queueWitness Native为{kind:"control",items:[原queue]}；DSH为{kind:"inbox",projection:{sequence,value}}。不归一化替换原source，不编造message.id。hold与queued均在重连后保留原identity；DSH仍不宣称通用文件下载或模型已读正文。

独立provider只允许hold、queued mixed、另一个session的无正文mixed三个业务请求，以及DSH已核实标题请求；后续请求history只可剥离经过精确核对的原hold user/固定final assistant，不能宽松跳过human。原images/files/MIME/顺序及ImageIO、原公开history/read恢复门禁继续执行。服务退出和provider停止后复核无重试/重复才写accepted。

主会话统一Cargo/实际harness/集成/CI与回填。复用SSD shared-target，不新建target。新源和程序在运行前冻结，失败与最终证据完整保留；不以本切片代替完整Composer/Transcript/原生/ProcessContainer交付。

生产修复范围：主会话发现Composer原queue reconciliation漏Native message.source.rpcId。定向真实公开DTO测试已红（accepted.submissions[0].reconciled为false），本轮在原回显退休分支补带user角色/source.kind守卫的读取；同scope原RPC才能确认，跨session/错role/错source/错rpc不退休，迟到失败不恢复已入队草稿。保留原顶层/Pi legacy路径，不另建队列状态机。真实Native probe还需确认同ticket已reconciled且无provisional echo。

Provider hold 细化：前两轮挡住整个HTTP响应时，Native只有inventory running，尚未生成原user/turn公开来源，因此两轮明确失败。真实busy流应先发送公开协议合法的起始事件和ENDPOINT_部分文本，flush后再设置hold_entered；独立queue-ready通过才发REPLY_OK及terminal。保持原final完整ENDPOINT_REPLY_OK，不放宽原committed user/running/queue事实门禁。
