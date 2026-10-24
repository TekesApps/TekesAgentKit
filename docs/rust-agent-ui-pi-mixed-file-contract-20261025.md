# #16：Pi mixed file/image 的 receipt 与来源契约

承接 `ef9991b` 的[真实三端图片闭环](rust-agent-ui-inline-image-live-results-20261025.md)。只读对照 ClientKit `c289bc9` 的 PIHostEndpoint.submitPrompt 和 PITranscriptProjection：文件描述保留在 input receipt，再映射为 timeline.fileAttachments；不伪装成原生消息内容块。

AgentKit 的文件选择、原 RunBinding、Composer admission 与上传 receipt 继续使用原实现。Pi `materialize` 从同 session 的真实 upload_record 产生 message/images/files，files 为 `{contentIndex,precedingImageCount,file:{attachmentId,name,bytes}}`；位置来自原输入枚举，不能从文本/path反推，receipt 不包含本地路径。真实文件 bytes 计入原 16 MiB 消息预算，单文件 8 MiB 和最多 8 附件不变。

原 direct/queue 的 input.prepare 携带这份 files；原 prepared/admitted FIFO/hash 状态机保持。JS 在 prepare 前整组验证并深复制，只在实际匹配的 user message_end 写入原 inputReceipt。旧请求缺 files 使用空列表，旧 receipt 形状保留。queue handled 清理原 admission，不为没有 native user 的命令编造 receipt；commands/run 的附件扩展语义本切片不覆盖。Child 仍明确拒绝 file。

文件事实必须符合：最多 8；标准 hyphen UUID（大小写接受且保留原 ID）；非空 basename、无控制字符；bytes 正整数、不超过 8 MiB，文件总量不超过 16 MiB；contentIndex 严格递增、无重复，precedingImageCount 非降且不大于实际原生图片数，缺省为 0。可选 facts 坏组整体忽略，原 RPC reconciliation 和原消息继续恢复。新数据不能创建读取或发送权限。

presentation 保留原 item/record/source identity、raw attachments、content_blocks 和 native block index。文件只存独立 receipt 来源与类型化 metadata；`admitted_file_attachments()` 读取、`attachment_display_order()` 合并附件相对顺序。同 image slot 多文件按 contentIndex 保留，不价值去重，不声称恢复 Pi 已合并的原始文本块顺序。InputMessage 只用原 SDK 详情控件消费额外文件卡，并给它独立的 receipt slot，保留原消息及原图片卡 ID；图片位图仍等待 TekesUI #117。

验证分层：Rust materialize/预算/跨 session；JS 原 receipt 的旧新兼容与快照；projection 两前缀/FIFO/坏组/source字段不变；独立公开 UI consumer 的顺序与原控制身份；真实 Pi 原 RunBinding 的 direct 与实际 queued mixed 输入、provider、公开 get_entries/history、重连。缺任一层就保留 not_run，不以单测代替真实队列或完整 UI。仍不跑桌面、截图、物理 IME/OS AX，不改 SDK/Kernel/ClientKit；复用 SSD shared-target。

实际分层结果、真实队列及重连见[验证记录](rust-agent-ui-pi-mixed-file-results-20261025.md)；这是 Pi main-run 切片，不代表整个 Composer/Transcript 或三端 mixed 已完成。
