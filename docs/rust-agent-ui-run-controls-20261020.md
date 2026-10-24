# 原生取消、模型失败与显式恢复

基线仍为已发布 AgentKit v0.3.8，UI 为 PR #10 的实验增量。TekesUI 固定到 `8cf0840790334ba224afc0595897c74231c712e6`，没有修改 Kernel、TekesUI 或 Swift 源码。

## 共享失败事实与渲染

真实 Pi 原生探针和组件回归确认：源回合已 error，但 Indicators 只显示空闲；Pi 公开的 assistant.errorMessage 在归一化时丢失，空 content 也使错误不可读。现在在 stopReason=error 时保留公开 errorMessage，取消时的 aborted/errorMessage 不被当作 provider failure。

presentation 公开只读 `TranscriptItem::failure_message()`、`TranscriptSnapshot::latest_failure()` 和借用类型 `TranscriptFailure { turn_id, message }`。仅最新规范回合明确为 error 才返回当前失败；没有说明就保留 None，不从 idle、文字内容或缺失 usage 推断失败/成功。旧错误保留在历史中，新 running/completed/aborted 回合不再把它作为当前失败。UI re-export 该类型，第二公开 consumer 可独立消费。

`IndicatorFacts.last_turn_failed` 来自共享失败事实，`run_error` 优先使用当前规范错误说明，保留原显式 runtime error 的 fallback。idle 且有明确失败时显示“执行失败”，没有说明也能显示已知失败。OutputMessage 在原 assistant 行内增加独立的 Status 文本控件展示错误说明，空正文不再吞掉真实错误；部分正文与错误可同时存在。不发明第二状态机、虚构时间或错误内容，布局继续机器靠左、用户及 footnote 靠右，样式来自 TekesUI。

DSH 的同一 inventory 流还会发出运行错误通知。内存投影现在只在该流收到显式 running=true 时清除该会话的旧 operational error；idle、历史 snapshot/refold、分页与其它会话均不清除它。独立回归先复现旧错误粘滞，再验证清除范围；历史失败条目始终保留。

兼容性：新 metadata 与 presentation 只读 API 为增量；未发布 UI 的 IndicatorFacts 新增 last_turn_failed，手写字面量须补该字段。没有改变 RunPhase 的既有枚举或 endpoint 的 mutation/retry 语义。

## 原生隔离探针

独立入口：`--components --transcript-content --endpoint-config <owned-blank-sessions.json> --run-controls-native-selftest`。仅用于两个空白、可写的自有会话；与其它 native selftest/offscreen 互斥。聚合入口增加 `--ui-native-run-controls`，其余 runtime/demo 配置与流式专项一致。两个窗口场景不会并行；每项探针使用独立的新会话。

1. A 的预置输入通过真实 Send 点击发送；等待真实 CANCEL_PENDING 预览、规范 can_cancel 和挂载的 Running 指标，再点击正常 Cancel。
2. 等源确认 aborted、can_cancel=false，检查只提交一次、A 草稿清空、B 中文草稿不变。点击 Reconnect，等新 epoch、非 stale 历史，再检查没有重发。
3. 通过原生文本服务输入新的错误测试消息，等待草稿和布局更新后再计算并点击 Send。本地 provider 返回确定性 HTTP400；不修改连接、不注入 endpoint 的假成功回执。
4. 检查最新回合 error、已挂载的 Indicators 失败事实和可读 Transcript 错误。通过真实原生 AX 查询该回合公开错误说明的 UTF-16 范围。Kernel 仅提供净化后的说明，不要求它暴露原始 HTTP payload 或测试标记。
5. 通过正常 Composer 显式发送第三条恢复请求，确认 completed、旧失败不再显示为当前失败，历史错误仍保留。再次重连，确认三条输入各一条，B 没有收到输入或丢失草稿。

探针由真实状态、原回调和原生事件唤醒，不强制连续重绘。45 秒单次 deadline wake 与外层55秒 timeout 保留失败；网络的受控慢流只属于本地合成 provider，不使用收费模型。

## 失败记录与验收边界

首轮探针把输入后的 Send 坐标提前算好；Composer 布局更新后点击到了旧位置，超时。改为等待原生文本写入、草稿与布局更新，再计算按钮位置。第二轮真实 Pi 明确复现 Indicators 的 run_error=None；对应规范化与共享显示问题有独立的先失败回归，随后修复。DSH 的后置 runtime-context/time-context 都可能为 user role，测试 provider 只取最后一个 user 时未识别慢流；fixture 现在查找最新显式测试标记，恢复标记优先于历史错误，并以脚本测试覆盖。没有放宽“真实预览后取消”、aborted、失败可读或重连不重复的断言。

本项是定向合成 AppKit 点击、原生文本服务和 AX 查询证据，不是物理输入设备、真实输入法候选窗或 VoiceOver 验收；不覆盖全部错误类别、队列/工具期间取消、进程崩溃或性能。阅读位置、分页锚点与富内容仍待 TekesUI #22/#25，完整目标尚未完成，UI 保持 0.0.0 / publish=false。

增加原生错误文本查询时，探针一度假定新只读控件光标在 0，实际组件按既有 external-value 策略放在末尾。查询改为核对当前投影的折叠选区，并独立查询错误标记的真实 UTF-16 范围；没有改变控件选区策略。随后 DSH 显式恢复的旧 operational error 粘滞被真实窗口断言捕获，按同一 inventory 流的明确执行信号修复，未用 UI 隐藏错误。

只读参考 ClientKit c289bc9d33edcd83a782ac17927a8c65a027eccd 的 PITranscriptProjection：stopReason=error 标记 failed，aborted 单列；SessionEndpointRuntime 的 agentError 是独立运行通知。保持这些事实区分，未修改 ClientKit 或 Tekes 的现有 WIP。

恢复阶段还遇到一次过早验收：输入记录已到达，但对应最终回复尚未发布。探针现在同时等待同一最新回合的规范 final assistant，不用 idle 或用户记录存在来推断恢复成功。Native 的标记过滤残留经诊断确认（真实控件已含净化消息）后修正，诊断运行主动中断，不计通过。

最终同一二进制的 Native/Pi/DSH 三端专项全部通过，详见[结构化收据](rust-agent-ui-run-controls-20261020.json)。root 166 项、UI workspace 285 项（含独立公开 consumer）、Python 26 项、Node 5 项测试通过；root/UI严格 Clippy、fmt、边界和实际八包199输入校验通过。三端 reportedError 保留各自公开说明，不假装 Kernel 返回了原始 provider JSON。
