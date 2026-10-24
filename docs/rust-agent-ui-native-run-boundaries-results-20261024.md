# #17 / #21：Native 显式 run 边界修复结果

按[契约](rust-agent-ui-native-run-boundaries-contract-20261024.md)修复原 Native projection 的显式边界，关联 #17/#21/#27 与 PR #10。只读对照 committed ClientKit c289bc9、Tekes8858ccd；[结构化收据](rust-agent-ui-native-run-boundaries-results-20261024.json)记录输入、程序和验证范围。没有修改外部仓库、引入 Swift 或另建运行状态机。

已有显式 started 的 run 不再因非 streaming 或出现下一 run 而推断完成。原 settled 按字符串 run ID 配对，只结算对应 turn、停止其工具并记录其最后 fork boundary。后续 user/assistant 仍归最近 started；重复 settlement 最后位置/时间生效，取消累计优先，失败不被后续正常消息覆盖。缺/非字符串 ID、未知或 orphan settled 不生成幽灵回合。保留当前 ClientKit 对空字符串 ID、预扫描配对和无 marker legacy 的语义；不自行增加协议限制。

新 worker 初始红测为 **1通过/6失败**，候选修复后8通过。主会话新增 marker-only 回归发现候选会生成无消息空回合，修正后不再制造 rail/navigation 目标；同时补强工具停止、未结算 run 无 ended_at/fork、首输入与 inner 输入归属及无效 started ID。最终 **10/0**。独立只读审查没有发现上述身份、结算、归属、工具或 legacy 的确定问题。预览继续沿用当前 ClientKit 的 activeTurnID 行为，不声称 matching-settled 与实时 preview 的交错已被真实 producer 验证。

最终源码在 Clippy 提示下把嵌套 if 改为等价 let-chain，随后完整重跑：root **371/0**、feature presentation **279/0**、feature headless **4/0**、UI **750/0/1原有忽略**。root/UI严格Clippy、格式、边界、验收矩阵、普通demo构建、新独立公开consumer执行均通过。consumer137文件closure为 `e216110c5e2c26b279f832c9c2cbfa596f004f640b23cf6560b0acfb743e4466`，运行后与准备输入一致；之前仅准备而未执行的pre-lint fixture不算验收。实际Cargo解析的SDK243文件closure仍为 `3ad8aa98a567679b775e78b634f45888c9eb70ffd05708a4f781c0ce7264471c`。Python源码未改未重跑，前一提交CI221项另记。

新 feature headless SHA256 `6aad53eb9910cb190ed5e887d27b1dd28169fea3245c1ed8f5eb27cb46a1215a` 在启动前归档后，用既有固定 Pi1.3.0 / DSH0.2.1-rc.2 重跑真实协议：**18+21条公开命令、15次合成loopback请求，两端 running→failed accepted**；相同child、父/tool来源与epoch配对，gate显式释放而非timeout/abort，headless与ownedDSH退出0。Pi原dispatching通过公开childRead自然退休，没有清队列制造终态。固定运行时的Node/入口/package等文件前后哈希一致；旧Pi包备份本身没有Node，预检拒绝后改用原完整固定目录，未安装或替换运行时。

这次真实协议是常规回归，不能把合成测试中的缺settlement、错ID或迟到结算称为已由真实producer逐项触发。没有新增Native Kernel生命周期、窗口/布局/glyph、桌面、物理IME、OS AX或ProcessContainer验收。也未证明服务端跨流事务一致性或完整Transcript对齐。

证据位于SSD `.runtime-fixtures/native-run-boundaries-20261024/`：1070文件源码输入、lint修正增量、候选补丁、红/绿与Clippy失败日志、最终测试程序/独立consumer、真实协议程序及隔离数据完整保留。最终定向程序SHA256 `3250c95862e86156354e35279e7050a54589df116a7c526657fb1cad109184ca`；otool只有系统dylib，归档程序不创建桌面宿主身份。所有构建串行复用SSD共享target，本轮删除0 bytes。

公开TekesUI仍固定50ec49f。核查时main7c13ec0只新增无关图标调整，#47/#48/#49/#53/#54尚无对应的新公开交付；不升级依赖或修改SDK。6318bf6的两条CI终态已回填原收据：root/UI/独立consumer通过，唯一失败仍为#53严格阅读；新提交CI另核对。#17/#21/#27保持开放，严格门不豁免，不合并/tag/发布。
