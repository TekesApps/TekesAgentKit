# 思考模式：受控 OutputMessage 接线（2026-10-23）

> 来源顺序契约已按具体 ClientKit projector 修正，公开来源帧工厂也已补充；见[来源适配记录](rust-agent-ui-thinking-source-20261023.md)。以下输入哈希和当时验证范围保持原身份，不将其当作最新完整交付证明。

在 `41249c8` 的共享思考策略上，新增 `TranscriptThinkingFrame` 与 `OutputMessage::update_with_thinking`。现在独立机器消息可以消费共享策略并更新原 SDK 控件；**Run/Transcript 自动来源适配、TranscriptContent/RunPort 接线仍未完成**，不把独立控件测试当作整个 Transcript 或真实三端验收。

## 输入和所有权

`TranscriptThinkingFrame::read(scope, turn_id, publication, epoch, display_revision, input)` 从上一阶段的明确来源事实生成 owned 展示帧。`publication`/`epoch` 必须来自同一个原 RunSnapshot 的 `revision`/`connection_epoch`；显示设置或 elapsed tick 只递增独立 `display_revision`。

帧绑定原 scope、turn、Run publication、connection epoch；保存按原 item ID 查询的 Hidden / Placeholder / Disclosure 决定。源事实指纹包含 settlement、live owner、final output、item/call/narration 及其原 arrival/frame 信息，排除偏好和 elapsed seconds。因此，同一业务 publication 可以改变模式和占位点数，却不能夹带新的来源事实。

`OutputMessage::update_with_thinking(publication, epoch, output, section, frame)` 使用组件自己的 scope；它不跟随当前选中会话。缺失真实 reasoning 来源、外部 scope/turn、不同 publication/epoch、递减展示修订、同修订冲突及同 publication 来源变更均在更新控件前拒绝。第一次接入受控帧后，原 `update` 不能跳过策略重新显示正文；之后每次都必须传递匹配帧。

Frame 是展示事实，不是 endpoint、CAS、发送或复制授权。AgentKit 仍需在原 Run/Transcript presentation 中补首次到达与 response-frame 的来源适配；宿主只接收既有 publication 和受控偏好，不得用时间戳、当前向量位置或 native 分页 ordinal 拼造顺序。Pi 的有界 event buffer 也不能作为永久首次到达账本。

## 控件行为

- Hidden 移除推理正文/disclosure，不把隐藏推理换成“内容缺失”提示；保留源 DTO，不删 reasoning。
- Placeholder 只安装一个原只读文本状态，使用 SDK Caption role 和共享本地化入口 `OutputMessageText::Thinking`；无推理 disclosure 或正文复制目标。重复 reasoning blocks 不产生多份状态。
- Disclosure 复用原 OutputPart 的 SDK Button/TextField、展开行为和身份；active 使用“思考中”，结束后使用“思考过程”。原 fallback 展开/收起文案保留，默认指示/chrome 仍受 #38/#47 约束。
- 切换到隐藏会退休旧 disclosure/body ID，旧排队动作无法操作替代控件；重新允许展开时新控件从折叠开始。已有可见推理的内容更新/结束/重连仍保留原控件、选择和焦点。
- 原 text/body/tool/attachment 的已证明块顺序和工具事实保持原路径；本次不新增 carrier 合并或平面排序规则，也不修改默认限高策略。

一次实际失败暴露了占位点数更新的版本问题：同一 Run publication 的新文字被原 SDK external-value 版本拒绝。修复为仅给被动 Thinking status 的只读文字使用独立、单调的控件值版本；原 Run publication/epoch 和业务 action 权限继续由父控件校验。版本耗尽在预检拒绝，不以修改业务 revision 绕过 SDK 事务。

## 验证与边界

根 workspace 267 项通过；完整 UI workspace 581 项通过、1 项既有 #49 忽略。新增覆盖包括 2 项共享 Frame 检查、8 项原 demo 控件测试，以及第二公开 API consumer 的实际挂载/展开/隐藏用例。所有测试使用既有公开 SDK 控件，不引入输入、滚动或渲染实现。

检查包含：同 publication 模式切换；旧展开/复制动作退休；占位 tick 保留 owner；冲突/旧修订/外部 scope/epoch/publication 拒绝；源 DTO 和指纹门禁；缺失来源拒绝；被替换推理无空提示；settlement 后 None 模式可展开保留推理；compact 结束时保留 editor/selection/focus；新 epoch 的展示修订重置。根/UI strict all-target Clippy、格式及依赖边界检查另见机器记录。

首次测试编译的 SDK 宿主构造错误、控件值版本失败及展开文案预期修正均保留原日志；最后全量结果对应修复后的输入哈希。没有运行新的原生窗口、IME、真实 SEP、阅读锚点或跨进程验收；先前 frozen 程序的成功不转移到本次代码。

## 后续完整接线

下一步仍需在 AgentKit 原 presentation 的来源入口保留真正首次到达与 response frame，关联 provisional→committed identity；然后把整帧模式/展示修订传给 TranscriptContent/RunPort，并把它们加入 prepared-plan 检查。必须验证前插/拆分/合并、同 publication 刷新、会话切换/重连和隐藏旧动作，不由宿主缓存或另写状态机。

自动来源适配完成后，再补 demo 的真实发送/流式场景、第二 consumer 的整体 RunPort 和三端原生验收。Compact 内嵌阅读仍须通过 TekesUI #53；本次不忽略、不放宽严格 gate，也不复制 SDK 逻辑。

[机器记录](rust-agent-ui-thinking-output-20261023.json) · [共享策略](rust-agent-ui-thinking-policy-20261023.md) · [完整交付清单](rust-agent-ui-delivery-status.md)

本轮复用现有 target、关闭 incremental，未新建 target、未复制验收程序或删除缓存。原源码、WIP、运行依赖、日志/收据和冻结程序保留。
