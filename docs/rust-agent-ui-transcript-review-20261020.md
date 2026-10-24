# Transcript review：非法历史隔离与空容器

本轮修复当前实验 UI 分支，不改变 SEP wire 结构、Kernel 或 TekesUI。AgentKit 基线为已发布 v0.3.8；UI 仍为 0.0.0 / publish=false。TekesUI 继续固定公开 `8cf0840790334ba224afc0595897c74231c712e6`，八包 199 输入，不读取兄弟工作树。

## 结果与接入

`TranscriptDocument::read` 拒绝可见消息中重复或倒序的内容块 index（允许间隔），以及同一 turn 中为空或重复的 footnote action ID。数据错误不能变成不明确的工具顺序或动作目标；有效数据的序列化格式不变。自定义只读子历史在 binding 返回可渲染 frame 前也经过 document/event-order 校验。

公开 `ReadOnlyTranscript::validate_update(&input)` 可在替换宿主缓存前预检；`update` 使用同一预检。测试宿主先丢弃较旧 publication/epoch，再检查数据、消息角色及已挂载组件能否采用。非法新 frame 保留最后有效历史，显示原 pane 的错误，允许用户通过原 source 的新手势显式刷新；失败不会触发自动重试。过期且非法的回包也不会把当前 pane 改成错误状态。

`OutputMessage` 仅在 Assistant 的正文、推理、附件皆为空且规范化 content_blocks 明确为 `Some([])` 时保持 inactive，不绘制缺失正文提示。后续正文仍能激活原节点。legacy `None`、状态/错误消息及不完整数据继续可见；不通过旁边的工具、时间或相同文本猜测空消息。用户消息右侧气泡和外置 footnote、机器内容左对齐继续成立，外观继承 TekesUI。

## 验证与证据边界

- Root presentation 完整测试与严格 Clippy；UI workspace（包括第二公开 API consumer）、严格 Clippy、fmt、依赖边界和固定输入核验。最终数量与日志摘要见相邻 JSON。
- 回归验证非法块、角色改变、缓存保留、显式刷新恢复、过期非法回包，以及明确空容器与未知旧数据的区别。重试测试先执行挂载时已经排队的刷新，再验证一次新的用户手势，避免把旧任务误计为重试。
- 最终原生 fixture 窗口通过六项组件检查及两项 thread/footnote 检查。它调用原生文本服务的合成 Preedit，不代表真实中文输入法或实体键盘验收。较早一次原生检查观察到 `xian` 而非注入文本，另一项失去文本焦点；失败日志保留，原因未确认，最终单独运行通过不能消除该证据边界。
- 真实 Native/Pi/DSH 独立 transport 使用本地合成 provider，验证恢复、分页、重连、纯附件及 GPU 内容树。Native 有界子历史保留 source-call 详情与原读取意图；真实小 child 没有更早页，不声称它证明分页锚点。对应主历史检测到一个明确空容器被隐藏，已检查截图。

本轮最终二进制 SHA-256 为 `ce9921442757aae7783836a3b60dd166353347b1f9f6a3f1cff9a7e788ddcfb6`。本地日志与截图位于 `/tmp/agentkit-transcript-review-*`，是本机临时证据；相邻 JSON 保存检查摘要与收据，不依赖临时路径长期存在。

仍未完成：TekesUI #22 混合内容阅读/分页锚点、#25 可选中富内容（本轮查询仍 OPEN）、真实中文 IME 和完整三端原生 UX、其余 footnote 能力、最终 review/CI/合并与 tag。节点/选区保留、GPU 截图和合成输入均不能代替这些验收。
