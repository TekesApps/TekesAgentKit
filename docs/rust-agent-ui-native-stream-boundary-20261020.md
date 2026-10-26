# Native 临时回复与持久消息边界

本轮公开 NativeConversationSnapshot 回归复现了数据覆盖：已持久化的 assistant 与新临时回复时间戳相同时，旧代码从 occurrence 计数减一，再以相同 Item ID upsert，替换了原持久行。现在临时回复使用当前窗口下一个未使用的 occurrence，不再从时间戳推断其对应某条持久记录。普通首条 live → committed 的底层 Item ID 兼容性保留，持久文档身份继续使用 NativeEntry。

另一个边界是 assistant 消息结束时 Agent 可能仍在执行工具。此前 Pi 将 message_end 转为 conversation 刷新，却丢掉了触发事件；投影仍能读到旧 message_update，留下过期预览。现在刷新帧可同时携带原始 `event` 原因，MemoryProjectionStore 验证后一次安装 snapshot 与事件。展示投影寻找最新 assistant 生命周期：message_start/update 提供预览，message_end 使其退场；toolResult 的 message_end 不会误停 assistant。消息的非空 stopReason 可以结束已持久行的 reasoning 状态，但不会把整个仍在执行的 turn 标成 completed。

这是 AgentKit Pi 适配器内部的增量帧规范化：没有新增 harness RPC、Kernel 接口或另一套 Run 状态机，NativeConversationSnapshot 的结构和原生 entry ID 不变。无 event 的旧刷新帧仍可读取；原 native-event 有界队列保持 256 项。附带事件类型不合法时，在修改任何会话投影之前拒绝。该原因不携带 journal sequence，也不是流/持久记录身份的阅读别名。

只读参考当前 ClientKit PITranscriptProjection 的 message_start 计数、committedAssistantIDs 与 message_end/agent_settled 刷新路径，以及 Tekes 的 transcript window 对原身份的使用。没有修改参考仓库或引入 Swift 工具链。

五个新回归覆盖持久行与同时间戳新流同时保留、正常提交后的 live ID、assistant end 与仍在执行的 turn、其他角色 end 隔离、原因与快照同次安装，以及坏原因不部分修改状态。真实三端测试在 Pi 发送结算后、重连前读取公开 snapshot，额外断言实际收到 assistant message_end 原因及其 canonical assistant 记录；不是仅用手写帧宣称适配器已通。

首次真实断言过早：权威快照已经读到最终回复，通知游标却还停在 system/user 事件。保留了失败与事件类型诊断；测试驱动随后增加只读 waitNativeEvent，等待保留队列中明确的 assistant message_end，再采样快照。它不把“数据已结算”等同于“通知已经全部到达”，也不承担 UI 状态或重试。

临时 native 流仍不具备公开 durable entry ID。其文档 key 不用于持久阅读锚点，不按正文或 timestamp 猜测迁移。正式 Transcript/RunView、流/记录阅读关联、混合文档分页锚点、物理 IME 等完整目标仍未验收。机器内容与 footnote 靠左、用户输入与外部 footnote 靠右、视觉继承 TekesUI 默认样式的约定保持不变。

验证：root 137、UI 185 tests（含第二公开 consumer）、工具 18 tests、两套 strict Clippy、fmt、依赖边界和实际固定输入检查通过。最后的只读观察命令增量另通过 headless 两项测试和 strict Clippy。真实 Pi 的新生命周期标记以及三端分页/重连/文档 key 离屏证明全部通过，最终 UI 二进制通过五项原生 fixture 回归。[机器收据](rust-agent-ui-native-stream-boundary-20261020.json) 保留源码/二进制和分层证据。没有将这些局部结果提升为正式消息布局、物理 IME 或阅读锚点验收。
