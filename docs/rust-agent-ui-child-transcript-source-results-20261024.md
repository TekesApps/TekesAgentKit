# #17 / #27：真实子 Run Transcript 来源结果

按[契约](rust-agent-ui-child-transcript-source-contract-20261024.md)完成父Summary之外的子Run运行/失败两帧，关联PR#10；[结构化收据](rust-agent-ui-child-transcript-source-results-20261024.json)记录程序、原帧与边界。只改Python验收，不改Rust/UI/SDK/Kernel/Swift或生产接口。ClientKit c289bc9与Tekes8858ccd仍作为只读基线。

`--child-transcript-source`显式追加两次child presentationSource，默认流程仍为Pi18/DSH21条命令。原父running见证先通过，再在provider held时验证原子Run running帧才release；原结算/Pi childRead0之后分别验证父failed和子error帧。三份父见证、两份子见证独立保存，不把不同scope/publication称同帧。

每个原User按Native entryId/ordinal/recordId或Journal seq/sourceSequence配对，再以完整opaque turnId关联唯一原TranscriptTurn。两帧scope/epoch/输入来源/turnId不变，revision递增；错误项、endedAt与fork boundary精确绑定原错误/结算来源。actionables只读同份authority，消息必须是整条记录位置、可见且非provisional，拒绝歧义项身份、缺错误、旧输入或尚未退休的失败队列。没有从文本或turn ID结构推导来源。

新worker初版主会话审查纠正了不存在的Transcript.scope、错误role枚举和opaque ID推导；修正后14项通过。主会话补充拒绝测试曾得到19项中4失败，补齐上述来源约束后21项通过；最终全Python **242/0**，diff/边界/验收矩阵通过。候选、原红/绿日志保留，不以自建fixture通过代替真实验收。

首轮真实`verified`中Pi两帧通过，DSH直接选择child后的原SessionProjection.summary为None，验收器拒绝。该字段在原Rust契约为Option，原Journal、run_open及错误边界仍完整。仅在子Transcript profile允许DSH这一公开缺省并记录inventorySummaryPresent=false；不填空对象或running=false，不放宽默认父Summary/Pi或摘要存在时的检查。缺省不改原witness的回归已验证。原失败收据、stdout和提取的rejected帧保留。

最终`final/run-receipt.json` accepted：**Pi1.3.0 20条命令，DSH0.2.1-rc.2 23条命令，共15次合成loopback请求**。Pi子Run在epoch2中revision45→55、DSH在epoch5中revision84→94，均running→error；原同一User来源和opaque turnId稳定。DSH运行帧库存摘要存在、失败帧缺省，如实记录。父Summary原running→failed仍通过；gate显式释放，无abort/timeout，headless与ownedDSH均退出0。固定运行时与程序前后哈希一致，无付费模型、私有历史或桌面。

复用当前共享target程序SHA256 `6aad53eb9910cb190ed5e887d27b1dd28169fea3245c1ed8f5eb27cb46a1215a`，启动时核对已有同SHA归档但不执行归档。Rust/UI/Cargo输入与f7fe185不变，137文件consumer closure仍为 `e216110c5e2c26b279f832c9c2cbfa596f004f640b23cf6560b0acfb743e4466`。因此复用该提交已验证root371/feature279/headless4/UI750/0/1及demo/独立consumer证据，本轮**没有调用Cargo、重新编译或新建target**，不能写成这些构建又运行一遍。

f7fe185的PR40096979427/push40096974552终态原artifact/log已核对并回填旧结果：root/UI/consumer/Python221通过，唯一失败仍为TekesUI#53严格阅读，后续CI demo跳过；新提交Python242与CI另验。精确TekesUI50ec49f不变，不豁免门槛。

证据在SSD `.runtime-fixtures/child-transcript-source-20261024/`：1075文件输入、可选库存修正增量、候选/失败/最终原帧、运行时输入和二进制复用收据完整保留。缓存删除0 bytes。只证明同帧内存来源一致和同次运行的时间顺序，不证明服务端多流事务一致或producer防伪；也不代替失败后恢复/分页、Native新生命周期、原生子pane操作/草稿、最终布局/glyph、桌面/物理IME/OS AX/ProcessContainer。#17/#27继续开放，不合并/tag/发布。
