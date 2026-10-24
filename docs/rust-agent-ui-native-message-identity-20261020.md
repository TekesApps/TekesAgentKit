# Native Transcript 的持久消息身份

分页回归复现了一个文档 key 错误：较新窗口里 assistant 的 timeline ID 为 timestamp:0；前插另一条同时间戳的历史后，同一持久 entry 的 timeline ID 变成 timestamp:1，timestamp:0 则属于另一条回复。两条回复即使正文完全相同也必须保留独立身份。此前真实长会话使用不同时间戳，未覆盖这个反例。

只读对照当前 ClientKit PITranscriptProjection 的 assistantItemID 和 assistantOccurrences：Rust 的底层 Item ID 保留了相同语义，本次不修改它或 stream reconciliation。Tekes TranscriptVisibleWindowTests 对原 section/window ID 的使用也说明阅读目标不能根据当前数组位置猜测。新的文档 key 优先级为公开 NativeEntry → 用户 Submission → 原 Item。NativeEntry 借用原 source_position.entry_id，不用 timestamp、正文、entry_ordinal、窗口大小或本机生成的 ID。

真实 Pi 的文档 key 验证又复现了输入回执边界：分页前看不到回执时 key 为 NativeEntry，分页后读到回执曾变成 Submission。新增公共快照回归复现该失败；现在持久 entry key 始终保留，TranscriptInput.reconciliation_key() 单独返回明确的 Submission 关联，供正式视图在同一原 scope 下关联乐观输入。该关联来自现有共享回执，不靠 UI 按正文猜测。

同一可见 native entry 不得重复出现在两条消息中，即使其中一条使用 Submission key；空 entry ID 也会失败。Submission 和 NativeEntry、Item 分属不同 key 命名空间。所有 key 仍由原 RunScope / pointer authority 限定，不能跨会话或换源复用。

公开 NativeConversationSnapshot 的回归先复现旧实现失败，再验证前插后的 retained entry key 不变且不绑定 older entry。另覆盖独立校验重复/空 entry、独立 Submission 关联和相同正文的临时流/持久记录边界。第二公开 consumer 通过 components 重导出的真实文档 API 验证窗口变化，不依赖 demo 私有数据。

临时 native stream 没有公开 entry ID 时，文档保留临时 Item key；它不是持久阅读身份。落盘后文档会使用 NativeEntry，不能仅凭旧 timeline ID、同一 timestamp 或相同正文创建阅读别名。正式视图仍需可靠的流/记录关联证明与 TekesUI 阅读事务；本次不声称该迁移、阅读锚点、正式 Transcript/RunView 或物理 IME 已完成。用户输入与 footnote 靠右、机器内容与 footnote 靠左，以及继承 TekesUI 默认视觉的契约保持不变。

最终验证：root 132、UI 185 tests（含第二公开 consumer），两套 strict Clippy、fmt、依赖边界和固定输入检查通过。三端真实长会话的分页/重连证明现在核对文档 key 和对应内容，不再只核对底层 timeline ID；Native/Pi/DSH 全部通过。最终固定二进制也通过五项原生 fixture 回归。失败与修复后的证据分别保留，[机器收据](rust-agent-ui-native-message-identity-20261020.json) 记录源码、二进制、测试与范围。真实三端仍是本地合成模型/隔离状态，原生 fixture 仍不是物理 IME 或正式 Transcript 验收。
