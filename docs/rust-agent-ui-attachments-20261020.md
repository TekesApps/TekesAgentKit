# 通用文件附件：共享绑定第一步

本轮实现 host 可消费的上传绑定；不是系统选择器、完整附件 UI 或三端附件闭环验收。
AgentKit 最新已发布基线仍为 v0.3.8；这里新增的字段/API 尚未发布。
TekesUI 固定 `431093786de47b48cd566d0d9c0eafa088b10d68`，未修改上游或依赖 pin。

## 已实现

- `RunBinding::supports_attachments / begin_attachments`：仅可写、连接 Ready、公开 upload/policy 方法均存在时支持；打开 host 选择器前捕获原 binding、destination、context revision、connection epoch。
- `AttachmentFile`：宿主提供不可变字节、basename 和 MIME；拒绝路径、控制字符、空文件及超过共享 `MAX_UPLOAD_BYTES` 的文件；Debug 不打印字节。
- `AttachmentSelection::upload`：一次显式尝试；读取真实 policy，检查文件数、单文件和聚合大小，逐个调用现有公开 `session.uploadFile`；RPC identity 仅作关联，不假定 Pi/DSH 上传幂等。
- 回执写回：只在全部成功后追加 `{type:"file",receiptId}`，通过原有 content CAS 保留新正文/settings。已返回的有效回执在部分失败、源变化和 Send 后保留在 report；失败不自动重传、不自动重新绑定草稿。
- `RunController::patch_context_at_epoch`：持有 canonical core 与 authority 读锁期间同时检查 epoch/Ready 和 context CAS，避免检查后重连再写入的窗口。锁顺序与现有 refresh 相同。
- 可选 `AttachmentFacts` 保存字节/MIME 元数据；旧序列化附件没有 facts 时仍可读取。旧附件大小未知时，聚合策略检查明确失败，不按零计算。facts 不进入 prompt；ComposerContext 仅显示已知字节数。
- 第二公开 consumer 可调用 `upload_selected_files`，不导入 runtime 内部、demo、路径或系统 API。

## 证据范围

确定性 SEP fixture 测试覆盖真实公开 policy/upload RPC 和规范回执，包含：原会话收件、新正文保留、B 草稿隔离、内容 ABA、Send 后晚回执、重连失效、部分失败/禁止重传、文件数预检、无能力入口、错误 destination、已关闭 SideChat 不回退父会话。
独立 policy 测试覆盖 inline MIME 与普通文件语义分离、未知旧附件大小、单文件/总量限制、无效本地 metadata 和字节 Debug 隐藏。共享 presentation 测试覆盖旧 JSON、字段往返、prompt 不泄漏 facts、原子 epoch 提交。

完整命令与计数见同名 JSON 收据。没有运行本轮系统文件选择器、原生上传窗口或真实 Native/Pi/DSH 附件发送；此前三端协议和 Composer 证据不扩展为这轮新增上传绑定的三端验收。

## 待完成

系统文件选择、受限文件读取、上传状态组件/窗口 wake、来源隔离的 pending controls、预览与失败对账 UI、内联图片路径，以及真实三端上传→草稿→发送→数据回传和原生窗口验收。demo 附件入口继续禁用，避免显示尚未接通的系统操作。

上传中断/取消 future 不能代表服务端取消；宿主需保留请求和文件并按未知结果显式对账。对端可能已存文件但未返回可用 receipt；Kit 不伪造 ID，也不自动删除远端文件。完整 Transcript/RunView 与阅读/分页仍受 TekesUI #22/#23 正式公开消费门禁约束，本轮没有替代通用控件或缩减总目标。
