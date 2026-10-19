# 无 UI Run / Transcript / Composer / 连接状态

日期：2026-10-18。实现位于 `tekes-agent-presentation`，只依赖公开 contract/session、Rust 数据和异步原语；没有 TekesUI、Swift、SQLite、窗口或控件。Components 仍未接入。

## 公开接口和权威边界

- `RunScope` 显式携带 endpoint/thread/receiving session；不拆解编码后的 UI ID，也不把 Pi entry ID 当成 journal seq。`ComposerDestination` 将现有 Run 与未建会话的 workspace 草稿分开，key 使用无歧义 framing；pane/file/diff 等视觉上下文不产生新草稿。
- `TranscriptSnapshot::read` 把一个已验证 `SessionProjection` 转成 turns/items/tools；原始 journal/文件仍由 SEP 拥有。`PresentationStore` 保存草稿、提交和本地 echo，并生成 `RunSnapshot`。同一目标的宿主表面应共享同一 store/controller；没有全局 singleton 或隐式磁盘存储。
- `RunController` 观察 `SessionRuntime` 的 revision，提供 watch snapshot、select/open/edit/prepare/dispatch/cancel/respond/load_earlier。Drop 只停止 presentation 观察，不停止宿主 runtime。宿主通过 `Arc` 共享 controller；选区是 controller 自己的状态。
- `SubmissionPreparation` 是显式宿主回调，收到不可变的提交目标、草稿 settings 和实际 session scope。模型/preset/权限、凭证、文件 staging 等产品能力由宿主在这里准备；默认实现拒绝非空 settings，不静默忽略。不存在自动模型选择或凭证读取。

## 对照 ClientKit 的行为

| 层 | 已实现行为 | 来源 |
|---|---|---|
| Run | loading/idle/running/waiting/interrupted/archived/unavailable；actionable/queue/jobs 读取及 scope 检查；interrupted 可输入但不显示运行控制，archived 禁止输入 | RunScope、RunView、ThreadStatusPolicy |
| Connection | connecting/connected/reconnecting/degraded/failed/stopped；端点健康与单会话错误分开；历史可保留 stale，断线不显示可停止/可提交/可回答 | SessionEndpointRuntime、RunMirrorObserver |
| Transcript | turn/step frame 稳定身份；preview 到 durable 替换；reasoning 结束边界；turn/end 才提升 final；明确的 commentary 不抢 final；whole-turn usage 替换消息求和 | SessionEventProjector、TranscriptReasoningLifecycle |
| Surface/tool | human surface replacement 原子校验；model-only replacement 不删除 human history；隐藏 runtime/developer/system 上下文；工具声明、执行结果/失败/中断及 opaque view；turn 结束停止未完成工具 | SessionEventProjector |
| Pi | native ancestry/opaque record ID；timestamp + occurrence 稳定 assistant frame；官方 message_update 的完整当前消息；原生 receipt 关联 rpcId；run settled/aborted marker | PITranscriptProjection、tekes-host public extension hooks |
| Composer | 按 destination 保存正文/附件/annotation/settings；A→B→A 的过期异步设置被 generation 拒绝；显式 command artifact 编译，不按正文首字符猜命令；保留 skills/references/annotations 和附件输入 | ComposerSurfaceContext、AsyncStateCommitPolicy、ComposerSubmissionCompiler |
| Submission | prepare 时捕获目标并清空已提交草稿；即时 echo；新线程创建后迁移目标和草稿；切换选区不重定向完成；新输入优先，失败附件先恢复；unknown 阻止再次提交直到明确 reconciliation | ComposerFailedSubmissionRecovery、ComposerSubmissionLifecycle、InstantInputEchoTests |
| Reconciliation | 只按 source.rpcId/receipt 或权威 queue rpcId 去重；相同文本不构成 acknowledgement；晚到失败不恢复已被权威确认的输入；取消已派发 future 归 unknown，未派发归 failed | TranscriptOptimisticEchoIdentity、submission lifecycle tests |

纯状态用例对照的是以上非 UI 行为，不声称完整 Swift projector 的逐字段移植。后续 todo/task/subagent catalog/SideChat/Summary 与有界子历史见 [headless-agent-panels](headless-agent-panels.md)。UI layout/scroll/disclosure/IME/a11y 尚未实现，不能靠给父 session 换个 thread ID 取得子任务 authority。

## 提交和观察

宿主创建 `Arc<SessionRuntime>` 并等待 ready，然后创建 `RunController`：

```rust,ignore
let controller = RunController::new(runtime.clone())?;
let scope = RunScope::main("native", native_session_id);
controller.open(scope.clone()).await?;
let destination = ComposerDestination::run(scope);
controller.edit(&destination, ComposerDraft { text: "hello".into(), ..Default::default() })?;
let ticket = controller.prepare(&destination, PromptMode::Queue)?;
// 此刻 snapshot 已含 echo；宿主可以切换选区，ticket 的目标不变。
controller.dispatch(ticket, None).await?;
// subscribe() 是最新完整不可变 snapshot 的 watch，不要求消费者逐帧接收通知。
```

NewThread 提交要求宿主显式提供 `CreateSessionRequest`（Native 的 cwd/execution 配置沿用公开 runtime 契约）；已有 Run 禁止再传 create。模型等 settings 使用 `dispatch_with(..., &host)`；没有 settings 时可用默认 `dispatch`。取消尚未派发的 ticket 使用 `cancel_prepared`。对 unknown，宿主核对权威历史后可调用 `acknowledge_reconciliation(id)`；接口不代表重试许可，也不会触发自动重发。

Pi companion 只通过官方 `input` / `message_end` / `agent_start` / `agent_settled` 和 `appendEntry` 记录 `agentkit.inputReceipt.v1` / `agentkit.run.v1`。receipt 经过已准备的 RPC 输入关联再绑定 native timestamp/hash，presentation 最后按 receipt 的 rpcId 退休 echo；不拿两个显示文本直接匹配。历史中的 ClientKit `tekes.*` receipt/settled marker 同样可读。图片经过原生扩展后使用实际文本 hash；有界队列派发也走同一 receipt 路径。JS companion 没有变成 UI 或私有 RPC。

## 验证与限制

新增 17 项 presentation 测试，workspace 共 59 项。`crates/presentation/tests/state.rs` 覆盖 identity/finality/replacement/usage/abort、Pi receipt、scope、compiler、恢复和连接状态；`tests/controller.rs` 验证真实异步等待、切换选区、取消和宿主准备失败。公开 consumer 包括 `tekes-agent-headless` 的 `compose` / `presentation` 命令及 `presentation/examples/presentation_replay.rs`。

真实 Kernel 0.9.0、Pi 1.3.0、DSH 0.2.1-rc.2 的隔离合成 provider 验收增加了 Composer → echo → admission → settled Transcript → identity reconciliation → reconnect 的断言。运行步骤沿用 `scripts/test-real-session-endpoints.py`。

这是内存状态实现：重建 controller 会丢失其本地草稿/echo；宿主应保持 controller/store 稳定。完整已加载历史按选中 session 重折叠，尚未做增量投影优化、大历史/RSS/120Hz 性能验收。watch 丢过渡通知时仍读最新状态，submission events 仅保留最近 256 项；本地 draft/submission registry 未自动淘汰。没有 UI、生产 launcher、付费模型、TLS 远端或 Linux 运行验收。源码基准在 `presentation-source-baseline.json`，其中明确区分当前 ClientKit 已提交源码和本地 WIP 参考。


## Live output usage (v0.3.5)

`TranscriptTurn.liveUsage?: TranscriptLiveUsage` exposes a real provider-reported
`outputTokens` cumulative turn count. Kernel already emits `assistant/chunk`
with `chunk.type=usage` and `chunk.usage.outputTokens`; no Kernel change is needed.
Per-step reports replace previous values (including zero), and partial input-only
reports preserve the last known output count. Sealed assistant usage replaces its
live step instead of double-counting it. Turn settlement, a new turn, journal
reset and stale authority prevent live values from leaking into another view.
No text length, tokenizer estimate, elapsed-time counter or synthetic increment
is used. Missing values are omitted. A provider which reports only at the end
cannot supply per-token ticking; actual report cadence remains authoritative.
Pi's conversation adapter currently has no live usage report. DSH usage chunks
pass through unchanged when reported by its assistant stream; neither path
fabricates availability.

Validation includes cumulative/duplicate/partial/invalid usage, sealed-step
replacement, multi-step totals, late reports after settlement, transient cursor
isolation, reset, a Web service subscription test, and the independent public API
consumer `cargo run -p tekes-agent-presentation --example live_output_usage`.
Existing formatting and Clippy findings in thread actions/annotations were also
resolved so the standard workspace gates run cleanly; policy behavior is unchanged.
