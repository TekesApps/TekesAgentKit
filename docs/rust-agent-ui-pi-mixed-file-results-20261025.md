# #16：Pi 混合文件与图片的原链闭环

对照 ClientKit `c289bc9` / Tekes `8858ccd`，承接 `ef9991b`。实现边界见[契约](rust-agent-ui-pi-mixed-file-contract-20261025.md)，机器记录见[JSON](rust-agent-ui-pi-mixed-file-results-20261025.json)。UI 继续是实验状态，仅消费公开 TekesUI `50ec49f`；上游及其 WIP 未修改。

## 改动与行为

原 Pi materialize 从同 session 的 upload_record 产生文件 facts，真实文件 bytes 与图片共用 16 MiB 消息预算；原文件路径提示格式保持。direct/queue 的原 input.prepare 携带深复制的 files，实际 user message_end 才写入原 inputReceipt，不另建 admission 状态机。缺 files 的旧 receipt 形状保持，handled 清理原 admission。

presentation 沿用 timestamp/hash FIFO，保留原 item/record/source、raw attachments 与 content_blocks。可选文件组坏掉时，原 RPC reconciliation 继续恢复，文件组整体忽略。文件的 contentIndex/precedingImageCount 只说明附件相对顺序，不假造原生文本块位置；额外文件卡复用原 SDK disclosure，用户气泡与 footnote 的布局沿用原组件。

## 实际验证

- 根工作区 388 项通过；UI 工作区 781 项通过、1 项既有忽略；严格 Clippy/格式通过。
- opt-in：presentation + headless 299 项、bindings + demo 667 项通过。这些计数与默认套件重叠，不能相加当作总覆盖。
- Python 285 项、Node 14 项通过；新增 projection 6 项、独立公开组件 consumer 4 项，覆盖坏组、FIFO、两 receipt 前缀、同槽同名文件、原控制 ID 与来源字段保留。
- 独立 standalone consumer 编译运行通过：138 个 AgentKit 来源文件，SDK 仍为 8 包/243 文件公开闭包。
- 真实 Pi **1.3.0 固定基线**，两个隔离 session，原 RunBinding 上传 `中文混合.txt`（29 bytes）并发送 text/image/file/image；direct 与真实 queued 均通过。队列 hold 是第三方本地 provider 的实际未回复请求，只有原公开 queue fact 的 scope/rpcId/id/内容通过校验才放行；不是仅凭 mode=queue。
- provider 恰好 3 次请求（direct、hold、queued），没有重试；四张不同的 2×2 PNG 的原 bytes/MIME/顺序匹配，保存原数据和实际 ImageIO 转码证据。文件作为原路径提示传给模型，不声称 provider 已读取文件正文。
- 原公开 session.fileAttachment 在重连前后回读同一文件 bytes/name/ID；原公开历史的 inputReceipt、user 原文/图片、最终 assistant 和来源 ID 保留。重连 epoch **2→5**；两个会话的草稿隔离通过。
- 当前 production 输入的 Native0.9.3/Pi1.3.0/DSH0.2.1-rc.2 原图片回归通过：6 次图片输入与 DSH 1 次标题请求分别计数。该回归不扩大 mixed 范围。

当前严格嵌套 reading 仍 **exit 1 / accepted=false**（TekesUI #53）；没有豁免断言。图片位图控件仍等 TekesUI #117。桌面、系统中文组字/OS AX、ProcessContainer、真实模型理解、Native/DSH mixed、其他图像格式/大图，以及 Pi child 文件和命令附件扩展不在本次通过范围。

## 证据、失败与交付

SSD `.runtime-fixtures/pi-mixed-files-20261025/` 保留各轮日志、provider 观测、公开来源收据、源码和程序哈希。早期进程退出检查曾返回 EPERM，随后只读检查未发现该组；改为有界正常退出等待，不提升权限。工具也修正了 quoted 文件提示、嵌套 receipt schema，并改为等待带原来源的 committed held input，避免把 provisional 当作队列前置证明；旧失败不计通过。

最终 `final/accepted-receipt.json` 在本次 headless/probe 均 exit0、无遗留 owned 进程组、provider 停服和最终请求计数复核后才写入。`final-programs/` 与 `final-source/` 固定本次程序及 551 个代码/配置/测试输入；`three-endpoint-regression/` 另存原图片回归。前期收据只按各自程序 SHA 解释，不转移为最终程序证据。

本次始终复用 SSD shared-target，没有新建 target，也没有删除缓存（释放 0 bytes）；源码、WIP、固定 runtime 和验收证据保留。PR #10 保持 draft，#16/#17/#28 仍开放；新 commit 的 CI 单独核对，不以本地结果代替 CI、合并或完整 UI 验收。
