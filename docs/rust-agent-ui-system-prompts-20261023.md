# System prompt 与开场输入来源

此阶段对照 ClientKit `SessionEventProjector.swift` 的有效 prompt fold、request header 和
`TranscriptMetalActivity.swift` 的 `initialPromptNodes/systemPromptRows`。
AgentKit 在现有 journal projection 上生成来源事实；不修改 Kernel、TekesUI 或 ClientKit。

## 契约

`TranscriptItem::system_prompt()` 返回可选的 `TranscriptSystemPrompt`：
`placement` 是 `Initial`、`Update` 或 `Inline`，`anchor_sequence` 是来源 fold 的显示锚点。
原 `source_sequence/source_position` 保留实际记录位置，两者不能混用作 durable identity 或访问权限。
旧 System 项没有该事实时，不从正文、时间或“看起来在前面”推断 Initial。

- system prompt 只拼接直接 text blocks，元数据不会成为提示词正文。
- 显式 append 后已有非空 prompt 则是 Update；普通 system 记录和 request header 依照原 turn/step 起点决定 Initial/Inline。
- request header 按原 same-step append、reason、startsSeries 和 effective text 变化去重。
- 模型侧 sequence replacement 更新有效 prompt，并按替换映射追踪存活位置；不会擦除已显示的 append-origin 卡片。非 system 替换可恢复前一存活 prompt。与来源一致保留同一模型位置的多个节点，空替换不能覆盖仍存活的 prompt。
- replacement 边界早于已知来源且无映射时，停止猜测有效 prompt；已显示历史仍保留。完整历史恢复后由原 projector 重建。
- 未知可忽略事件的同名 surfaceOp JSON 保持不透明，不参与 prompt 替换。
- 没有当前 turn 的 request prompt 为 session-level card，不创建虚假 turn。

Initial prompt 作为独立阅读行放在用户输入之前，使用原 OutputMessage 的 SystemPrompt disclosure；
默认为折叠，展开只读内容沿用 TekesUI TextFieldContent。Update/Inline 留在 Activity 时间线。
本地展开由原消息控件管理；宿主不复制输入、IME、焦点、路由或滚动算法。
普通和分段 Activity 不能隐藏 Initial prompt。正式限高嵌套阅读仍受 TekesUI #53 门禁约束。

## 开场输入身份

`TranscriptItem::input_role()` 返回 `TranscriptInputRole::{Opening, Inner}`。
Journal producer 显式传出它已经用于 turn.input 的 authored/non-steer 开场选择；
Native producer 使用当前 canonical turn 内的首个公开 user entry，空文本不会使后续输入覆盖开场。
事实在 `metadata.inputPresentation` 绑定 item ID 与 turn ID；更换任一身份后该事实失效。
它与 Web DTO 顶层的 `inputPresentation` 正文呈现字段不同，不是 session pointer、历史读取权限或未加载历史的证明。

共享 planner 把已证明的开场输入放在 Initial prompt 之后、Activity 之外，移动原控件而不复制正文或状态。
已证明的 Inner 即使前面没有机器内容，也采用输入间分段；只有来源不足时才保留原 legacy 路径。
多个 Opening 的冲突在整帧 preflight 拒绝。宿主不能用 turn.input 的文本相等、时间或行下标恢复身份。
同锚点 prompt 卡片按来源版本再按 ID 排列，保留 3→10 的顺序；显示锚点不是原 sink 顺序。

## 验证范围

本阶段程序、源码差异、日志与收据保存在 `.runtime-fixtures/system-prompts-20261023/`。
十二项来源测试覆盖初始锚点、header 去重、append Update、替换链、裁剪不确定性和 session card；
第二 consumer/宿主组件测试覆盖来源开场输入晚于工具时仍在 Activity 外，以及 Initial 顺序、原 disclosure、外层 Activity 折叠及 Composer preedit 保留。
最终验收程序保存在 `final-input-source/`，UI SHA-256 为
`6a3a49e75020042f00d47561bfec46a2cfafe33754de15d8225517f9c9684ec7`。
完整根 workspace **301 通过**，UI **602 通过、1 项原有忽略**；两套严格 all-target Clippy、格式和边界检查通过。
同一程序的 prompt/普通/输入分段原生 profile 均通过：原 prompt disclosure 实际点击，
随后工具/推理与外层 Activity 的展开、收起、再展开、resize/type-scale、阅读锚点及原文本 owner/Composer marked baseline 均保留。
这组 marked text 和 wheel 来自测试事件；一次正常 LaunchServices 系统软件拼音另单独通过，
不是物理键盘、候选窗或稳定 IME 验收。

新编译的 headless/UI 程序通过 Pi 1.3.0、DSH 0.2.1-rc.2、Native Kernel 0.9.3 的
stream、stream-reading、公共分页 native profile。三端使用真实 harness 和拥有的本地合成 HTTP provider；
分页分别 50→56、50→59、50→56，`nativePagingControl=false`，各原始 false flags 保留在 JSON。
Native 为明确 UI-only，不代表完整生命周期或有界子历史通过。
这些真实三端回归不等同于真实来源 prompt 卡片全组合；新 prompt 行为的原生专项使用公开 journal fixture。
旧输入分段程序的原生收据不转移到本阶段；早期静态候选的源码、哈希、日志和 review 失败证据保留；其中未运行的候选程序已按占用检查清理。

`42575ef` 的两次基础 CI 成功、两次 UI CI 均在严格 nested-detail step 失败（官方 job API）。
本阶段继续保留严格非零门禁，不合并或打完成 tag；新提交的 CI 单独检查。
当前仍只消费公开 TekesUI，复用 `/tmp/agentkit-layout-text-292-target`，未新建 target、未删除共享缓存。
程序 hardlink、固定来源和验收证据保留。清理 8 个无占用且无外部硬链接的旧静态候选可执行文件，唯一文件分配块合计减少 429,838,336 字节（409.9 MiB）；未归因并发文件系统可用量变化，详情在 static-program-retirement.json。

仍不宣称完整 UI 对齐。剩余 narration carrier/fallback、完整富内容/配对布局、正式 ProcessContainer #48、
严格嵌套阅读 #53、稳定系统 IME 与 VoiceOver 等继续列在交付状态中。
