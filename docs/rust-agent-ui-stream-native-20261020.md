# 三端流式输出的原生窗口探针

## 范围和运行方法

本项继续消费已固定的 TekesUI `8cf0840790334ba224afc0595897c74231c712e6`，不修改通用编辑、焦点、选区或滚动实现。业务 presentation 与公开组件来自 AgentKit；仅在 demo 测试宿主增加 opt-in 探针。初始基线为 PR #10 的 `254afde`，已发布契约仍为 v0.3.8。

独立入口：`--components --transcript-content --endpoint-config <owned-blank-sessions.json> --stream-native-selftest`。必须有两个空白且可写的自有会话，不能与 offscreen、其他 native selftest 混用；若会话已有消息或草稿，探针直接拒绝，避免覆盖。A 的初始测试输入是 `ENDPOINT_STREAM_NATIVE_PROOF`，B 有独立中文草稿；仅 A 会发送一次。真实 transport 的本地合成 provider 提供有间隔的 SSE，确保实际预览能被观察，不调用收费模型。

总入口是 `scripts/test-real-session-endpoints.py --ui-demo <built-binary> --ui-components --ui-transcript-content --ui-native-stream`，并提供对应的 `--pi-root` / `--dsh-root` / `--kernel-root`。脚本通过公开 endpoint 新建两个隔离会话，为 A 选择本地测试模型，分别打开原生窗口；不使用已有用户会话。可以指定多个 runtime，窗口按顺序运行。`--ui-output` 用于保存原先的 offscreen PNG 和各 harness 的 `.native-stream.log`，两类收据保持各自范围。

## 真实窗口检查

- 实际 AppKit 点击 Composer Send，通过正常 RunView/host callback 发送；不直接调用 runtime.prompt 替代按钮。
- 等待已挂载的真实 provisional assistant 正文，记录原输出行和正文 ContentId，点击正文后通过原生 NSAccessibility selector 设置 `0..4` 选区。
- 等待 authoritative final response 且对应行已挂载，再断言最终正文仍使用原行、原正文控件，选区和流式期间的文本焦点保留。读取实际原生 AX value、selected range、selected text、substring，并通过 NSTextInputClient 查询最终正文。
- 切到 B 检查其中文草稿，再切回 A，检查原输出行、正文控件和选区仍保留。
- 点击正常 Reconnect，等待新 connection epoch 和非 stale 历史，检查只有一个用户输入、A 草稿已清空、B 草稿未变，且历史正文仍保留原控件和选区。不会重新点击 Send 或自动重试 mutation。

推进依赖实际源状态、控件挂载和正常回调；本地动作 FIFO 零延时投递，不强制连续重绘。45 秒单次 deadline wake 和外层55秒进程超时只负责让缺失回调失败。

## 已修复的业务组件问题

DSH 的后续历史回补会在聚合正文和结构化内容块之间切换，旧 OutputMessage 以不同 slot 重建正文 TextFieldContent，导致原生 AX owner 和选区丢失。组件回归先复现 ContentId 变化，再修复为：在已验证的同一 assistant 消息内，只关联唯一的正文或推理角色；支持编码双向转换，保留正文/推理控件及展开状态。新旧任一方有多个同类块时不关联，不比较文本或猜测块 index。命名空间预检与实际复用使用同一谓词，保持非法输入先拒绝、后更新。

这属于 AgentKit 的消息组合身份处理；没有改动 TekesUI 的 editor、selection、focus、AX、layout 或 scroll。第二公开 consumer 独立消费该行为。

最终同一二进制的 Native/Pi/DSH 原生专项均已通过，详见[结构化收据](rust-agent-ui-stream-native-20261020.json)。三个窗口顺序运行，包含实际预览、规范历史、原行/文本控件/选区、会话草稿和新 epoch 重连的检查；原先 offscreen 收据仍保留 nativeInteraction=false。

## 失败记录与边界

初版 Pi 探针用较新的 current_run 判断结算，而窗口树尚未挂上对应规范行，断言失败。现在先等待实际规范行存在，再严格比较它与捕获的旧行；若另建了新行仍会失败。第二版在切回 A 后额外点击正文，原生点击主动把选区变成光标，随后查询失败。改为直接查询保留选区；没有放宽选区范围或更改基础控件。两次失败日志保留于结构化收据，不将其冒充产品修复。

原生点击是定向合成 AppKit 事件；AX selector 与文本服务是真实平台入口，但不是物理鼠标/键盘、真实输入法候选窗或 VoiceOver 验收。本探针只检查稳定控件及其文本选择，不验证向上阅读、滚动偏移、分页锚点、富内容或完整三端 UI。TekesUI #22/#25 仍需上游交付；UI 仍是 0.0.0 / publish=false，完整目标尚未完成。

DSH 的第一项输入计数失败包含两条隐藏的 runtime-context/time-context。已按公开 source.kind 区分其与真实用户输入；计数仍包括来源为空的 optimistic echo，因此不会借隐藏内部消息排除未确认的重复提交。随后两次 AX 查询失败保留；进一步直接观测到同一输出行的文本 owner 在回补后改变，并由组件测试独立复现。修复聚合/结构化身份衔接后，同一最终二进制的三端原生检查通过。未修改平台 AX 发布或放宽选区断言。

最终 UI workspace 282 项测试通过（含独立公开 consumer）、Python 脚本21项通过；严格 UI Clippy、fmt、边界和八包199输入校验通过。root 业务源码本轮未改，沿用上一提交164项 headless 与绿色 CI 的收据。只读 Kernel fixture 为干净的 ae5d6fdf32290207b646b8c99b96f2a002902cb3。
