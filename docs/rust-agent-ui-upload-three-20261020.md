# 附件公开绑定的 Native / Pi / DSH 验收

本轮验证既有 Main 附件 UI/共享绑定连接真实 harness 的行为，没有修改 Kernel、ClientKit、TekesUI 或 harness 私有文件。AgentKit 最新发布基线仍为 v0.3.8，新增 UI API 未发布；TekesUI pin 保持 `431093786de47b48cd566d0d9c0eafa088b10d68`。

## 测试入口

三端脚本新增 `--ui-components --ui-attachments`；demo 的 `--components --upload-proof --offscreen` 运行对应证明。该路径点击公开 Composer 附件入口，取得原 binding 的 AttachmentSelection；宿主用拥有的临时 `中文+a.txt` 测试文件和受限读取适配器代替系统选择器，然后走原上传绑定、实际 SEP policy/upload、公共状态组件、共享 Send 和实时历史。不是直接调用 prompt 绕过 Composer。

上传前切到 B，并保留 B 的新正文；A 的晚完成不能切回导航或写入 B。通过旧 request 再次执行 upload 必须在本地失败。真实字节回读只在公开 `session.fileAttachment` capability 存在时执行；另一会话使用同一个 attachment ID 必须失败。每次 Send 都通过公开组件捕获原 ticket，dispatch 使用既有 ModelSelectionPreparation/RunController，不写新的提交状态机。

先发送“正文＋附件”，结算后检查一条非 provisional 持久化输入及文件元数据/规范化引用。显式关闭终态状态后，以新的用户选择捕获另一次上传，发送“无正文、仅附件”；检查该输入仍可见、没有重复、草稿已清空。最后重连，要求连接 epoch 改变且原会话的新 journal baseline 已 Ready；直接以该新权威投影调用既有 TranscriptSnapshot::read，核对两种输入各保留原身份、各一条，再等待新 publication revision 的 UI snapshot。这样不会把旧 Ready watch 值冒充重连回补。B 草稿保持不变。

测试只使用隔离 HOME/dataRoot 和本地合成 provider，没有收费模型或用户凭证。owned 文件读取不访问 harness 私有数据库；通用文件读取也不会回退到图片 API 或工作区任意路径。

## 实际结果

| 实际来源 | 正文＋附件 / 纯附件发送 | 历史与重连 | 通用文件字节回读 |
|---|---|---|---|
| Kernel 0.9.1 Native SEP | 均通过 | 文件元数据，两个输入各一次，保持可见与身份 | 原 session 字节一致；其他 session 拒绝 |
| Pi 1.3.0 SEP | 均通过 | 规范化为文件路径引用，两个输入各一次，保持可见与身份 | 原 session 字节一致；其他 session 拒绝 |
| DSH 0.2.1-rc.2 SEP | 均通过 | 文件元数据，两个输入各一次，保持可见与身份 | 无公开通用文件读取接口；明确不支持 |

每种来源还通过 A/B 草稿隔离、一次上传尝试、公开卡片/Send、assistant 结算及数据到 GPU 路径。运行时使用现有 Rust Native/Pi/DSH 三套独立适配器，未改变它们的协议语义。

DSH 的 `session.attachment` 是 image-only，其公开 SessionAttachmentRequest/Value 注释与字段均声明图片；ClientKit 当前 DSHHostEndpoint 同样只有 uploadFile 和图片 attachment。工作区文件服务是另一个授权域，不能据此声称支持上传附件下载。JSON 中不支持时 `publicByteRead=false`、`readUnavailable=true`，跨会话字节读取结果保持 null，表示未执行，不冒充一次成功的拒绝检查。

已修正文档矩阵中 DSH “raw binary 上传和读取”的含糊描述，分别列出上传、图片读取和通用文件下载限制；UI README 的早期“文件选择/上传仍未接入”说明也更新为当前事实。

## 证据边界

先前 [真实 macOS 系统选择器](rust-agent-ui-upload-host-20261020.md) 仍是独立本地 SEP fixture 原生窗口证据。本轮三端证明使用隔离文件 provider 和离屏 GPU，不是三端系统选择器、物理 IME、VoiceOver 或完整原生窗口验收，也不是标准 Transcript/RunView。纯附件“可见”指共享 TranscriptItem.is_visible，不是正式消息视图已经实现。

第一次只验证正文＋附件的三端运行通过并保留日志；随后扩展纯附件、原身份恢复和重连检查通过；最后补强新 epoch/journal baseline 及 publication revision 检查，三端再次通过。完整命令、三端结构化 stdout 收据、binary/source/log hash 与版本来源见 [JSON 收据](rust-agent-ui-upload-three-20261020.json)。未把新测试的静态 true 字段当作证据；每个字段均在相应实际操作、数据和断言完成后才输出。

## 剩余工作

SidePanel 附件选择器、可用来源的预览适配器、专用失败/未知结果对账 UI、内联图片；DSH 通用文件下载仍受上游公开能力限制。正式 Transcript/RunView、阅读与分页锚点、物理中文输入法和完整宿主验收继续待完成。PR #10 保持草稿；本轮不合并、不打 tag。
