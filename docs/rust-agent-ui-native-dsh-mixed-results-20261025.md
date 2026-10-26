# Native / DSH 混合附件非桌面闭环（2026-10-25）

关联 #16、#17、#28，PR #10；[接入/验收契约](rust-agent-ui-native-dsh-mixed-contract-20261025.md)，[结构化结果](rust-agent-ui-native-dsh-mixed-results-20261025.json)。本轮只补验收工具和文档，未改生产状态机或上游。

Native Kernel 0.9.3、DSH 0.2.1-rc.2 的两个独立普通会话，分别通过原 RunBinding 发送有正文和无正文 image/file/image。每次都实际上传29字节中文名文件、捕获两张不同颜色2×2 PNG；原 prepare 保留原顺序，公开非 provisional user 绑定原 RPC，最终回复属于原 turn。这里在空闲会话使用 Queue 模式，不声称验证了繁忙队列；真实忙时队列属于已有 Pi mixed 结果。

原 runtime reconnect 的 epoch 均2→5，两条 user/最终 assistant 的原 ID/source/turn/附件保持；公开图片回读共16次，Native 文件字节回读4次，均在重连前后核对原 bytes。DSH 未声明通用 session.fileAttachment，只验证上传回执、原公开历史文件引用和 provider 的精确文件句柄；不声称下载文件或模型读取正文。

独立 synthetic provider 实收 Native Responses input_file 原名/text/plain/29字节与两张图片；DSH Chat image_url 原图片及原位置 file/image handle。共4个业务请求、DSH1个合法首消息标题请求，无重复或自动重试；8张provider图片实际通过ImageIO解码转码。最终收据在两端owned进程退出0、provider停止后复核计数才写 accepted。未使用真实付费模型、桌面、私有history捷径或TekesUI修改。

探针修正按公开源码：Native project_content_blocks 将持久 File block 转为 attachment/mediaType DTO，不能把持久asset格式当公开历史；DSH正常prompt可带runtime/time两条独立上下文，标题分类须先识别标题框架，时间上下文按已发布三行格式完整识别。补充了拒绝持久格式冒充公开DTO、未知后续human、重复标题human、错误OpenAI图片格式及完整provider校验链的回归。

验证：Python296（聚焦7）；严格example Clippy、构建、fmt、UI边界与验收矩阵均通过。生产代码未变，本轮未重复root/UI全量；对应前序提交的结果不能冒充本轮新运行。当前consumer源码138文件未因example改变。新提交CI另行记录，未通过前不计完成。

证据在 SSD `.runtime-fixtures/native-dsh-mixed-20261025/`：first至seventh失败、eighth中间通过、final最终通过，各轮程序/源码/日志均保留。最终555项输入哈希、程序副本、公开收据和图片证据保留。所有构建复用SSD shared-target，无新target，本轮缓存删除0字节。

源边界：最初一个子任务误读了owned fixture私有history，其候选已隔离且未采用；该尝试不是验收证据。后续替代实现只消费逐项允许的公开发布源码/公开API，实际runner不读取私有history。保留 source-boundary-receipt.json，不能声称整个探索过程从未发生误读。

UI仍试验状态：位图组件等待TekesUI #117，严格嵌套阅读#53与桌面/焦点/中文IME仍未验收。未合并、未tag、未关闭完整Issue。
