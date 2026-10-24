# #17：Native 显式 run 终态修复结果

已对照 committed ClientKit c289bc9 的 PITranscriptProjection 与 Tekes8858ccd host，修复显式 Native run 内 error 被后续 stop/toolUse 消息遮蔽而误判 completed 的问题。采用[契约](rust-agent-ui-native-run-terminal-contract-20261024.md)，关联 #17 / PR #10；[结构化收据](rust-agent-ui-native-run-terminal-results-20261024.json)记录具体输入、程序和验证层级。只改 AgentKit Rust，不改 SDK、Kernel、ClientKit 或 Tekes。

在原 settled 分支读取同 turn 已保留 assistant 的明确 stopReason，按原 marker aborted 或 assistant aborted → error → completed 选择原因。消息正文/errorMessage 不提供终态 authority；原行/turn 身份和后继 run 隔离不变。未增加状态机、没有改无 marker legacy 或 Journal 路径。

新 worker 的初始 4 项回归在基线全部失败，候选修复转绿。主会话复核发现候选把未 settled 的流式 turn 提前写为 error；新增边界测试实际失败后，改为只在原 settled 位置读取累计事实。候选补丁与失败日志保留，最终 **7/0**：error→stop、error→toolUse、aborted→stop、取消优先、后继正常 run 隔离、streaming 仍 running，以及普通诊断文本不制造失败。

最终 root **361/0**、feature presentation **269/0**、feature headless **4/0**、UI **750/0/1 原有忽略**；root/UI 严格 Clippy、格式、边界、验收矩阵、普通 demo 构建和新独立公开 consumer 执行通过。矩阵检查第一次命令写错文件名而退出2，已通过正确的 `check-agent-ui-acceptance-matrix.py` 重跑；两份日志均保留，不把调用错误记成产品测试失败或掩盖。Python 未重跑，本轮没有 Python 修改；上一提交的221项CI单独记录。

独立 consumer 当前137文件 closure 为 `c808063f7d046183a1979145fe201efc8334118969e8390ef1115e258a675823`，执行后与准备输入一致，stdout 为空、退出0。公开 TekesUI 精确 pin 仍为50ec49f。新提交 CI 单独检查，不能借用 1d6cc7e 终态；后者 root/UI/consumer通过且唯一失败为#53严格嵌套阅读。

证据 `.runtime-fixtures/transcript-run-terminal-20261024/` 在 SSD：1066 文件输入快照、红/绿日志、候选补丁、最终测试程序和独立 consumer 归档。最终定向测试程序 SHA256 `f36b47494a3af4a958cfc6a4d5fc00adbb68f5549c7e67129e442406c2759afa`，otool 仅引用系统 dylib；归档不用作新的桌面应用宿主。全部构建串行复用共享 target，没有新建 target，本轮删除0 bytes。

本轮是原投影的合成 Native 历史回归、编译和公开 consumer 证据，没有新增真实 harness 生命周期、窗口/glyph/布局、物理 IME、OS AX 或 ProcessContainer 验收。richContent/completeTranscript 标志未变，#17保持开放；#53严格门不豁免，不合并、tag或发布。
