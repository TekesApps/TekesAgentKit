# #25：Native Task 的原 Summary 来源见证

只消费原公开协议与固定 TekesUI。此切片复用 #25 的 `acceptance-source-witness` feature；不改 Kernel、ClientKit、Tekes、SDK 或 Summary 算法，不启动桌面。

## 依赖与输入

Runner `scripts/test-real-native-summary-source.py` 必须显式接收已构建的 feature headless、固定 Kernel providers 快照、已安装 Kernel0.9.3 supervisor/worker，以及SSD项目下新的证据目录。复用 `owned_native`、`provider_handler` 和既有 `native_task_seed_commands`；隔离HOME/状态与合成loopback provider，不使用付费模型、用户凭据或harness私库。仅消费现有程序，不调用Cargo。

## 来源与行为

公开 Task seed 生成一个父Run内的 Task、原 childCatalog 及只读bounded history。公开 `subagents.history(maxMessages=50)` 与重连前后 `childHistory` 都必须绑定实际父/child身份；这些读权限不能变成Summary Task独立Run导航权。本切片不调用fork，也不把历史Kernel fork失败当根因已修。

重连后公开open原父Run，再调用原 `presentationSource`：Snapshot、summary scope、selection、connection epoch/readiness必须与同次内存输入一致。原Task名称来自同帧原tool.arguments.task_name，status来自同一tool.phase；Task scope必须仍为父scope，不接受注入sessionId/independentRun。

来源位置按kind读取。当前Kernel0.9.3返回Journalseq：call/result各按原sequence、类型、原message与callId匹配。NativeConversation entryId/ordinal/contentIndex只用于相应kind的独立协议fixture，不能套在Journal上。opaque scoped toolId从不拆分。degraded/stale、错scope/epoch/位置、伪造pointer或替换原参数/状态均拒绝。JSON校验不能识别整个producer完整伪造；也不证明服务端多流事务一致。

原公开 consumer example使用显式 `--passive-tasks`：只接受Native父scope内的一个Task，将原facts送SummaryView→ContentHost，核对原TextFieldContent标题挂载、canonical_status事实、无导航action。默认Pi模式仍要求原actionable child，不因Task模式降低门槛。只证明挂载/语义，没有glyph渲染、窗口、OS AX、物理IME或完整布局验收。

## 验收与证据

保留commands/stdout/stderr/版本/程序hash/原witness及Kernel正常退出收据；只有ownedKernel退出0后写accepted运行收据，不干预其他进程。首次未识别Journal方言的校验失败及来源保留。实际结果另见本切片结果文档；本契约不能替代运行或据此关闭#25/#23。所有构建继续复用SSD共享target，历史程序仅归档。
