# Pi 实时预览与落盘阅读身份

基线仍为已发布 AgentKit v0.3.8；本变更属于 PR #10 的未发布增量。TekesUI 固定为 `8cf0840790334ba224afc0595897c74231c712e6`，未修改上游源码、Swift 或 Kernel。

## 协议与身份

Pi 1.3.0 的公开 RPC `message_update` 经 `modes/json-event.js::toJsonEvent` 转换后只携带增量与 usage，不再携带完整 message。旧适配器依赖完整 message，会漏掉实时正文。AgentKit extension 现在消费公开 `message_start/message_update/message_end` 事件，携带经 token 验证的过程内 UUID 与完整 partial。Rust 在对应 RPC update 到达时一次性取走 partial，归一化为现有 presentation 输入；旧版自带完整 message 的 update 优先使用原值。未认证、不同 lifecycle 或缺少 partial 的事件不能借文本或时间猜测关联。缓冲仅包含一个待消费 partial，不是第二份历史存储。

公开 extension 的 `message_end` 在保存前发生，不能当场读取 leaf 充当 entry ID。桥接器保存最终对象的引用，后续只读 `reading.identities` 命令从公开 SessionManager.getEntries 查找保存该对象的 assistant entry，以对象严格相等产生 `{streamId, entryId}` 回执。不使用私有 `_entryIdsByMessage`、时间戳、文本摘要或 occurrence。若对象被替换、出现多个候选或旧版本复制对象，则不给回执。最终对象引用缓存最多 256 个，不追加 session records，也不写入 harness 私有数据库。

回执只对当前 conversation/generation 中已加载的 assistant entry 有效。重复 stream/entry、空身份及非 assistant/未加载目标会在替换内存投影前拒绝；重连清除回执。`TranscriptMessageKey::Stream` 是临时阅读身份，不能用于 CAS、Fork、评价或历史定位权限。落盘后规范身份始终是 `NativeEntry`，输出通过 `reconciliation_key()` 暴露一次明确关联；后续读取不带回执也继续使用该 NativeEntry。没有回执时分配新控件，不猜测关联。

## 控件与验收边界

正式 TranscriptContent 使用既有原子 preflight 和 namespace 分配，按明确回执保留 OutputMessage 行及其结构化正文文本控件；不复制 TekesUI 的选区、焦点或滚动实现。布局不变：机器及 footnote 靠左，用户气泡及外置 footnote 靠右，样式继承 TekesUI。

组件测试覆盖行/正文 ContentId、焦点与选区保留、后续无回执历史、相同内容无回执时不关联、重复回执原子拒绝。第二公开 consumer 独立消费 Stream→NativeEntry。真实 Pi 探针在同一次 dispatch 中观察实际预览，再检查最终 entry 回执和原 message_end；合成 SSE 提供受控延迟，不调用收费模型。此探针是实际协议与内存投影证据，不是原生窗口或输入法验收。

早期实时探针出现超时，日志保留。首次串行等待以及后续并发等待均未通过；查阅已安装 Pi 的公开 RPC 转换实现后确认缺失完整 message，补齐 extension 归一化后实际预览链路通过。不能把早期超时仅归因于探针时序，也不能把仅结算回执通过当作实时预览通过。

Kernel/DSH journal 已有 `response:{turn}:{step}` 项目身份。本次不声称其所有 aggregate→structured 文本控件、物理 IME、完整三端原生 UX 或阅读锚点已验收。TekesUI #22 混合内容阅读事务与 #25 可选中富内容仍开放；向上阅读、分页锚点和富内容选择仍需上游能力与原生联调。UI 保持 0.0.0 / publish=false，完整交付尚未完成。

兼容性：新增 frame 的 readingIdentities 与投影字段保持 serde 缺省兼容；NativeConversationSnapshot 的字段不变。手写 SessionProjection 字面量及穷举 TranscriptMessageKey 的 Rust consumer 需适配新增字段/Stream 分支。

验证记录见[结构化收据](rust-agent-ui-stream-identity-20261020.json)：root 164 项、UI workspace 280 项（含第二公开 consumer）、Node 5 项、Python 21 项通过；root/UI 严格 Clippy、fmt、边界、实际八包199输入校验及 demo build 通过。

同一最终 demo 的隔离原生窗口 `--components --native-selftest` 六项回归通过，覆盖 Composer/Indicators、Sidebar/Summary、SideChat、Task、Child 和只读子历史；该 smoke 仍明确排除正式 Transcript、阅读锚点与物理 IME，不能升级为本次 Pi 身份关联的原生验收。headless `compose` 的可选 `captureNativePreview: true` 仅用于隔离协议探针，在同一次 dispatch 中捕获并返回 `nativePreview`，不重发输入。
