# 思考来源：原归一化链与公开帧工厂（2026-10-23）

后续整帧输入及 prepared-plan 接线见[整帧阶段](rust-agent-ui-thinking-parent-20261023.md)；本页保留来源阶段当时的验证范围。

在 `f5dfd8e` 的独立 OutputMessage 接线上，补充原 Session/Transcript presentation 的来源适配。现在可直接从同一不可变 RunSnapshot 或只读 PaneSnapshot 生成思考帧；宿主无需维护到达顺序或查询一次新的 runtime。**TranscriptContent/RunPort 的受控输入、prepared-plan 和布局接线仍未完成，本页不是完整 UI 交付或真实三端/原生验收。**

## 对照源码后的契约修正

前两阶段将 `arrival_order` 概括为不可随分页变化的“首次到达顺序”，这个概括过强。进一步核对 ClientKit 的 `SessionEventProjector` 和 `PITranscriptProjection` 后，实际规则是 dialect-specific 的 presentation order：

- Journal（Native Kernel / DSH）：实时 chunk 的传输 `seq` 可能为 per-attempt ordinal 或 cursor+1，不能直接作展示顺序。预览使用接收时最后一个 durable cursor + 1；提交的 assistant 使用其持久 seq 与同 step 首个 call seq 的较小值。call 保留其真实来源位置，frame 由公开 turn/step 关联。
- Pi NativeConversation：提交消息与 call 使用当前公开 ancestry 的相对顺序；实时消息位于当前 ancestry 之后。该顺序可以在新 publication 的历史扩窗后重基，但不能充当 identity、reading key 或权限版本。它不是 UI 行下标，也不是长度 256 的事件缓冲下标。

共享策略比较同一 publication 内的顺序和 frame；不新建统一 first-seen ledger，不按时间戳/正文关联身份。现有 `source_position`、NativeEntry/Stream/Submission reading identity 和历史 cursor 保持各自含义。这个修正取代先前文档对“禁止任何 native ordinal”的宽泛表述；不把有限缓冲的索引当作来源事实。

## 数据路径与 API

`TranscriptSnapshot::read` 在原归一化路径生成 `thinking_sources`，包含原 scope、当前 live turn 和按原消息/call ID 索引的来源事实。新增字段有 serde 默认，旧序列化快照不会被猜测或补造来源；显式请求思考帧时返回 unavailable。元数据独立装箱，保持原只读 pane 枚举接口，不放宽 Clippy。

`RunSnapshot::thinking_frame(turn_id, mode, display_revision, elapsed_seconds)` 从本 snapshot 的 `revision`、`connection_epoch`、`run.scope` 和 Transcript 构造 owned 展示帧。公开的 `TranscriptSnapshot::thinking_frame` 接收明确的捕获上下文与 `TranscriptThinkingDisplayInput`，也支持无 Composer 的只读来源。

`PaneSnapshot::thinking_frame(turn_id, input)` 分别转交 Run 或 ReadOnlyChild 自己的 scope/publication/epoch；Missing 返回错误。只读 child 的 publication 是读取调用顺序，不能用父 Run 的 revision 替代，也不需要伪造可写 Run。显示模式与 elapsed 的独立修订沿用上一阶段约束，不改变业务权限。

第二 consumer 的 `install_run_thinking_output` 展示实际调用：只接收不可变 snapshot、目标 item 和受控显示输入，在 Kit 公开 API 内生成帧并安装原 OutputMessage；没有另一个状态机、Session 查找或 IO。该 helper 是消费者示例，不是新增标准业务组件。

缺失 legacy 来源、外部 scope、live call 的来源不完整等情况显式失败；不要求宿主用列表顺序修补。当前来源适配面向已有公开数据；不能据此宣称所有裁剪/未知来源都已达到完整 Views 对齐。

## 修复的数据回传问题

1. **Journal reasoning-end**：有些 chunk 同时附带完整 `presentation`，原分支只更新正文而跳过结束标记。现在从公开 chunk kind 更新 phase，再处理完整 presentation，保留显式结束事实。
2. **迟到预览**：已提交 assistant/message、step/end、turn/end 或已开始新 turn 后，迟到 chunk 不得替换持久响应或重新产生旧预览。这个门禁在原 Transcript 归一化中使用现有公开关闭事实；不改 cursor，不发送/重试请求。
3. **Pi 缓冲淘汰**：当前 public assistant 消息不再依赖 bounded `native_events` 保存。原 SessionProjection 保存一个当前 `NativeAssistantProjection`，日志仍限制 256；没有新增磁盘 mirror。message_start/结束/切换/已提交 receipt/断连按原生命周期更新或退休它。
4. **Pi phase/owner**：thinking_end、text/toolcall 开始及终止事件保持正确 phase；已知当前 stream 不跟随别的 stream 的迟到 update/end。不用时间戳或相同文本猜关联。旧 event-only snapshots 保持兼容读取路径；明确 Idle 不会重新扫描旧记录恢复预览。

这是一份 harness 数据投影，没有模式 UI、展开状态、字体、焦点、IME、滚动或进程实现；TekesUI 仍拥有通用基础能力。

## 证据与未完成范围

根 workspace 276 项通过，其中新来源用例 9 项；完整 UI workspace 582 项通过、1 项原有 #49 忽略。原 demo 中的公开 fixture SEP → SessionRuntime/RunBinding → 同一 RunSnapshot → 第二 consumer → 原 SDK 控件用例通过，覆盖重连、草稿、原 editor/selection/focus 和只读 envelope 的独立 publication。

来源用例覆盖：transport ordinal 与 durable anchor 区分、完整 presentation 的结束标记、同 step tool 关联和提交锚点、历史前插、提交/关闭/新 turn/step 后迟到数据、Pi ancestry/时钟分离、300 条额外事件淘汰日志后的当前消息、旧 stream 回包、新 lifecycle 和断连退休，以及 legacy/外部 scope 拒绝。只读 envelope 是 API 值检查，不冒充真实 Native bounded-child 验收。

根/UI strict all-target Clippy、格式、依赖边界和固定 SDK 输入检查的终态记录见[机器记录](rust-agent-ui-thinking-source-20261023.json)。测试构造/抽取编译错误和尺寸检查失败保留原日志，没有忽略或降低检查。

本轮未运行新的真实 Native/Pi/DSH 模型协议、原生窗口、系统 IME、阅读锚点或跨进程验收；已有 frozen 程序收据保持原身份。下一步将上述帧接入 TranscriptContent/RunPort 的整帧发布、展示修订/旧 plan 门禁及业务布局，再做真实三端和原生续验。

TekesUI 仍固定公开 `50ec49f3446924fdbc66b3cff22cd8cfe150fe7c`，AgentKit 最新已发布基线仍为 v0.3.8；本分支 API 增量未发布。#53 和 #48 等门禁不变，PR #10 保持 draft。完整范围见[交付状态](rust-agent-ui-delivery-status.md)。

本轮复用现有 target，关闭 incremental；未新建 target、未复制验收程序或删除缓存。保留源码/WIP、固定依赖、测试/检查日志和原验收程序。

后续当前源码本地 fixture 原生阅读与系统拼音续验已完成，见[独立原生收据](rust-agent-ui-source-native-20261023.md)。上述本阶段未运行原生的描述仅指来源帧测试阶段；未扩展为真实 harness 或整帧思考接线验收。
