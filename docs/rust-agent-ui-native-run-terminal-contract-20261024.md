# #17：Native 显式 run 终态累计

只改 AgentKit，关联 #17 和 PR #10。对照已提交 ClientKit c289bc9 的 `PITranscriptProjection`：assistant 消息在所属 run 内累计 failed/aborted，settled 状态取消优先、失败次之、正常完成最后。Tekes 已提交 host 8858ccd 不另建这层状态机。两个外部仓库均只读，Rust 不引入 Swift 源码或构建要求。

当前 1d6cc7e 的 Transcript 显式 settled 分支只看最后 assistant，导致同 run 的 error 被后续正常消息遮蔽。沿用原 Native projection/turn 身份和 settle 路径，在当前显式 run 的原 assistant facts 中选择终态；不从正文或 errorMessage 猜失败。原 settled aborted 必须保持取消优先；assistant aborted 的累计按同一已提交 ClientKit 契约处理。后续独立 run 不继承前一 run 的失败，输入/输出/工具身份和内容不变化。

范围限显式 Native run，不修改无 marker legacy 的既有末尾处理、Journal、adapter、库存 raw terminal 优先级、pointer、Kernel 或 TekesUI。#25 的只读 thread activity 已按累计规则处理，本轮消除 Transcript 对应差异，不宣称实现完整 Transcript/ClientKit parity。

先对 error→stop、error→toolUse、aborted→stop、后继正常 run 补红/绿回归，再主会话检查 root/feature、UI、普通 demo 与新独立公开 consumer。测试和组件检查分开记录，不把定向合成 Native 快照当成新的真实 harness 生命周期验收。沿用公开精确 TekesUI50ec49f，严格 #53 门不豁免。

所有构建串行复用 SSD 共享 target，保留红/绿日志和输入快照。桌面、截图、物理 IME、OS AX、ProcessContainer 本轮不运行；没有 merge/tag/release 授权变化。结果另回填 #17，完整验收未齐仍保持开放。
