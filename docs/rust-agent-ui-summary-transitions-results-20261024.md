# #25 / #27：真实子会话 running → failed

本轮修复 Pi 已有公开失败历史却显示 notStarted 的缺口，并完成 Pi 1.3.0、DSH 0.2.1-rc.2 的真实子会话转换。关联 PR #10，采用[接入契约](rust-agent-ui-summary-transitions-contract-20261024.md)；结构化结果见[收据](rust-agent-ui-summary-transitions-results-20261024.json)。TekesUI 精确依赖仍为 50ec49f，未修改 ClientKit、Tekes、Kernel 或 TekesUI。

Pi 的原 NativeConversation 包含当前 run 的 error assistant 与匹配 started/settled marker，但库存 blank=true、status 缺省。thread_activity 现在只读选择已有当前 run 的明确失败/取消事实，对齐 ClientKit 的 run 内累积和取消优先规则；不改库存、pointer、adapter 或原 Transcript 状态机。新输入、新 started、截断边界和不可信身份不能复活旧 Native 失败；Native 存在时不回退旧 Journal。正常 stop 不推断 completed。8 项定向回归由红转绿。

最终 `sealed/run-receipt.json` accepted：Pi 18 条命令、DSH 21 条命令，共 **15 次合成 loopback 请求**，无付费模型或私有历史读取。请求保持期间先捕获原父 Summary running 和原 child run_open/authored input，再显式释放 HTTP 400，捕获同 child、同父/tool 来源和同 epoch 的 failed。Pi 原 dispatching 项在运行时为 1；通过公开 childRead 等待派发退出并确认 pendingInputs=0/running=false/settled=true 后为 0。未取消或重写队列制造终态。两个 gate 均 explicit release，未 timeout/abort；headless 与 owned DSH 正常退出 0。

Pi 当前 error assistant ordinal13 与 run 边界配对；DSH 当前 authored input seq32、最新 turn/end seq36 reason.kind=error。两端 raw status 都仍缺省。四份原 SummaryView → ContentHost 挂载通过，检查 running/failed 指标策略、原行标题和 action scope；没有窗口或 glyph 渲染结论。

验证：root **354/0**，feature presentation **262/0**，feature headless **4/0**，UI **750/0/1 原有忽略**，Python **221/0**；root/UI 严格 Clippy、格式、边界、矩阵、普通 demo 构建和新独立公开 consumer 执行通过。consumer 137 文件 closure 为 `bbf8e983c009a09841320fe1b70002d4ca544bbfdd382ecd839e8a45c92399da`；不能沿用上一提交的 closure。CI 增加 opt-in headless 测试，严格阅读门保持不变；新 head CI 另核对。

证据根 `.runtime-fixtures/summary-source-transitions-20261024/` 已位于 SSD。最终 headless SHA256 为 `9ccd461ebc95850fac2dbd659229bd48d3d1610aa87de8592fa646552c7a0b3f`，程序在首次正式运行前归档；`sealed` 引用同 SHA 归档，避免重复副本。原 Summary consumer、独立 consumer、1062 文件输入快照、原始失败和所有运行日志保留。初次 audit 的程序在覆盖前未归档，仅保留原 SHA、日志和明确标记的源码重建，不能宣称恢复了该 binary。默认 feature-off 的独立运行边界程序早于最终 Native 修改；最终默认编译由当前 root 测试验证，两项证据不混用。

中间 `verified` 因运行时原 dispatching 项被过严拒绝，`final` 因失败捕获早于派发退休被正确拒绝；均保留。最终 runner 通过原公开 childRead 排序而非放宽失败要求，`after-child-read` 与 `sealed` 两次通过，以 sealed 为最终收据。

只读复核没有发现新 Native 选择器内部复活旧失败的确定性问题。既有显式 raw terminal 优先级和 Journal-only 规则保持不变；producer 是否可能在新输入后残留旧 raw terminal 的跨来源一致性尚未证明，本轮不能据此宣称修复全部旧状态风险。

本轮未验证服务端多流事务一致性、producer 防伪、完整 Transcript 对齐、父草稿完整生命周期、最终布局/hover、桌面、物理 IME、OS AX 或 ProcessContainer。#25/#27 保持开放，TekesUI #53 严格门仍未放行，不合并、tag 或发布。本轮删除缓存 **0 bytes**，所有验证复用 SSD 共享 target。

CI 终态补记（1d6cc7e）：PR40094689268 / push40094685938 均已结束；root354、feature presentation262 + headless4、UI750/0/1、Python221 和独立 consumer 均通过。两份原 artifact/log 核对 consumer137文件 closure 与该提交源码一致，SDK243文件 closure不变，PR synthetic merge 父含1d6cc7e。唯一失败仍为严格嵌套阅读：accepted=false、parentOffsetAfterAppend=634、detailVisibleAfterAppend=false；后续 demo build 被跳过，本地普通 demo 构建另通过。严格门不豁免。
