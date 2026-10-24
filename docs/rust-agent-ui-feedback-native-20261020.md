# 三端评价控件原生窗口验收（2026-10-20）

正式 Transcript/RunView 的评价控件已在 Native、Pi、DSH 三个真实 SessionEndpoint 传输上，通过独立 macOS 窗口中的自动化点击和原生文本服务验证。本轮只增加 AgentKit 测试宿主与三端脚本的验收模式，没有修改业务组件、Kernel、TekesUI、Swift 实现或固定依赖。

## 可复现入口

三端现有临时会话验证脚本增加 `--ui-native-feedback`，要求同时提供 `--ui-demo --ui-components --ui-transcript-content` 及待验证 harness 的显式 root。脚本先验证原有 offscreen 内容树，再顺序启动每个原生窗口；临时 provider、会话与凭证属于同一次隔离运行，不使用真实模型收费调用。

单个窗口入口是 `--components --transcript-content --endpoint-config <owned-sessions.json> --feedback-native-selftest`。它必须使用显式会话配置，不能与 offscreen 或旧 fixture selftest 混用；应只用于拥有且允许修改的临时会话。需要独立可写 source 的公开 feedback.list/put/delete 能力。初读发现已有评价时直接失败，不覆盖既有用户评价。正常 demo 不运行该探针。

## 本轮证明的行为

- 两份初始 Composer 草稿均非空：仅在测试宿主的空草稿上设置本地样例，已有非空草稿不覆盖；进入组字前确认发送按钮确实可用。切换到 B 再回 A 时，原生文本查询分别核对当前编辑器的实际草稿，避免仅检查后台数据。
- 目标来自实际挂载的 Transcript 评价行，点击前检查其 native frame 中心仍在可见 clip 内。动作通过 TekesUI 公开 `selftest::Action::Click` 的 AppKit 鼠标事件进入原窗口，未直接调用组件意图或组装 feedback RPC。
- 刷新 committed records 后点击 positive，随后打开备注。原生文本客户端接受合成中文 marked text；尝试保存、关闭、发送和切换会话后，`ExpectText` 仍要求原 marked text 存在。提交中文文字后，再显式点击保存。
- 保存完成以实际 committed note 与编辑器关闭为前提，重开后要求恢复原备注。切到会话 B 再回 A，要求同一 retained widget 仍保留该编辑器和文字。
- 关闭备注后点击同评级撤销，等待 committed absence，再点击刷新并要求新 publication 仍为 absence。该流程结束后，两份 Composer 草稿与原 user message ID 列表保持不变，未发送 prompt。
- 只采用原始 RunView/RunBinding 的正常异步回调，没有自动重试、重投当前会话或额外反馈状态机。结束时走实际 CloseRequested 路径，所有断言通过才输出 `NATIVE_AGENT_FEEDBACK_SELFTEST_RECEIPT`。

网络等待按控件/committed 状态推进；本地动作以 FIFO 零延时投递。输入动作及原 runtime 回调触发下一帧，探针不强制持续重绘；单独 deadline wake 和外层进程 timeout 捕获缺失动作/回执。这个 probe 结构仅用于验证，不修改产品编辑、焦点、滚动或绘制算法。

## 证据与限制

UI workspace 277（含第二公共 consumer）、脚本 21 tests 及 UI strict Clippy、fmt 和依赖边界通过；root 业务源码未改，158 tests 的此前收据单列。[JSON 收据](rust-agent-ui-feedback-native-20261020.json) 分别保存三端 native receipts、原生阶段日志、固定 TekesUI 输入和构建证据。原有 offscreen receipt 的 `nativeInteraction=false` 保留原范围，不能被新的窗口证据回写成 true。

这里的 native pointer 是自动化合成 AppKit 点击，native text services 是 NSTextInputClient 路径；preedit 明确为合成。不是物理鼠标/键盘、真实候选窗或 VoiceOver 验收。此前独立系统拼音探针验证 Composer 的软件输入法链路，仍是另一层证据。失败、CAS conflict 与未知结果的细节仍由此前绑定/组件及真实协议测试证明，不把本次成功路径扩大成全部错误状态的原生验收。

初版探针曾强制持续重绘，并给下一步动作添加线程休眠；Native 的返回会话点击出现明显投递延迟并两次超时。去掉休眠后可完成但仍有延迟；进一步移除探针自造的持续重绘、改用输入/回执唤醒后，Native 单独复测和最终三端顺序运行均通过，原截止时间与行为断言未放宽。失败及慢运行日志保留；不据此声称已定位 TekesUI 或操作系统内部调度机制，也不将阶段耗时视为性能基准。

该项专项原生验收已补齐；完整三端 Run/Transcript UX、可靠 stream→durable 阅读身份、其余 entry/footnote 能力仍需继续。TekesUI #22 混合阅读事务与 #25 可选中富内容仍未交付，追加时的向上阅读、分页锚点和完整目标尚未验收。UI 保持 0.0.0 / publish=false。

最终二进制的 Pi/DSH 专项来自三端顺序运行，Native 专项来自随后同版本的独立运行。前一轮在 Native UI 启动前的归档检查返回 session-running，该前置生命周期失败保留，尚未确认其就绪判定原因；没有修改 Kernel 或增加自动写入重试。此后 Native 独立隔离运行的前置流程和原生专项均通过，不能据此抹去前一轮失败。
