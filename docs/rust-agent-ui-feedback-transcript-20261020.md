# Transcript 评价 footnote 接入（实验，2026-10-20）

评价现已挂入公开 `TranscriptContent`，并通过 `RunViewIntent::Feedback` 和原 binding 的一次性请求执行。它属于机器输出的 footnote 区域：输出正文、TurnFootnote、TurnFeedback 依次排列并靠左；用户气泡与用户 footnote 继续靠右。按钮、编辑器、字体、圆角、颜色和交互样式仍继承固定 TekesUI，没有新增视觉主题。

## 公开输入和事件

- `TranscriptContentLabels::feedback: Option<FeedbackLabels>` 或 `TranscriptContent::configure_feedback` 显式开启；已有 RunView 使用公开 `configure_feedback`。默认 None 保持兼容。第二公共 consumer 验证该入口，不依赖私有的 RunView 子树访问方法。
- 开启后，只有 `TurnFeedbackTarget::read` 能证明最终回复身份的 completed section 才分配 `TurnFeedback(turn_id)` 行。展示回退 ID、未结算/临时回复、不能证明目标的机器事件不会出现评价入口。
- `feedback_controls()` 返回当前控件 ID 与目标；`feedback_mut(id)` 用于原挂载回执采用；`take_feedback_intents(actions)` 返回原控件 ID 和已捕获目标。RunView 则返回 `Feedback { control, gesture: Box<FeedbackGesture> }`。每次事件或 AX 动作后立即 drain，包含纯文字/组字事件。
- 宿主用 `RunBinding::supports_feedback()` 识别独立 source 的公开读取能力，再开启 UI。该事实不代表当前 mutation 许可。初次 unknown 状态提供显式刷新；成功的 `FeedbackFrame::controls` 才能提供 committed presence/absence 和当时的 put/delete 可用性。宿主也可先读取并输入当前 committed frame。
- 手势产生处同步 `prepare_feedback_gesture(frame.as_ref(), *gesture)`，再把返回请求交给 executor；不在延迟任务中重新找当前选中的会话。网络完成后，对原控件调用 `FeedbackUiCompletion::apply`：Ignored 保留宿主当前 frame，Adopted 替换该原 binding 的 optional frame。

Demo 的 `component_feedback.rs` 仅路由原 pane/控件、共享 committed frame 和临时回执，不构造 endpoint payload，也不另写评价 CAS、Run 或编辑状态机。一次写入/读取认领旧 frame 后，其他输出的旧写入授权暂停；有效结果或显式新读取再分发给当前仍能证明目标的控件。

## 目标、挂载与生命周期

历史窗口 ordinal 或真实时间信息变化不会换绑同一回复。正文/推理/附件/块发生变化则分配新的 row namespace，旧控件 ID 不回收。正在组字的评价行若将被替换或移除，整个 Transcript 更新先拒绝，保留旧可读树和文本字段；结束组字后可接受新的 canonical frame。这里没有自己实现滚动或像素补偿。

连接 epoch 或 stale 历史使反馈授权失效，保留仍打开的备注文字，要求刷新。纯 Composer/备注组字状态不会被当成服务器授权变化来重置编辑器。RunView 和宿主导航均检查备注组字，组字期间不能保存、关闭、发送或切换会话。

切到别的会话后到达的回执只留给原 retained widget，回到原 pane 才采用，不导航、不替换新会话草稿；原行已被真正移除则丢弃。采用时再次核对原 binding、frame 和目标，而不只是在网络 execute 返回时核对。已完成的回执和宿主的 frame 槽位使用弱引用，关闭 pane 后不会因此保留 RunBinding 或历史观察。仍执行中的一次性请求继续拥有其已捕获 source，不自动重试或改投别的 source。

有界 Native 子历史不作为独立 feedback session：`ReadOnlyTranscript` 直接拒绝带评价控件的内容配置。独立 task/subagent/SideChat 则按其真实公开能力和原 binding 走同一条 RunView 路径。

备注行为沿用此前[独立控件契约](rust-agent-ui-feedback-controls-20261020.md)：trim 空白/BOM、空白清备注、失败保留草稿、冲突采用权威 current、删除评价关闭编辑器。同评级撤销，切换评级清除旧备注；已有 committed note 使用 TekesUI 的 selected 语义提示，不把未保存文字当作已提交备注。

## 验证与边界

UI workspace 277、其中反馈 22、内容树 14 tests，以及脚本 21 tests、UI strict Clippy、fmt 与依赖边界通过；root 本轮未改，158 tests 的此前收据单列。具体二进制、三端回执及原生记录见 [JSON 收据](rust-agent-ui-feedback-transcript-20261020.json)。新增测试覆盖真正挂载的 RunView/Transcript 路由、备注组字与发送/导航屏障、切换后的原控件回执、回复修正时的 ID 退休与组字前置检查、延迟采用失效，以及已关闭 binding 的释放。

三端脚本增加显式 `--ui-feedback-transcript`：在隔离 session 的实际内容树里通过公开语义动作执行初读、评分、中文备注保存/重开、同评级删除及重读确认，检查两份 Composer 草稿不变。GPU 布局检查评价行靠左，用户正文及 footnote 靠右。该探针只修改它拥有的临时目标；不覆盖原本已有评价的真实用户回复。

原生 fixture 回归与系统拼音探针单列记录：系统拼音证据验证 Composer 的真实 marked text 与一次显式发送；备注组字屏障目前是公开 ContentHost 合成事件测试。三端评价语义动作/GPU 证据不等于原生点击、物理键盘、候选窗或 VoiceOver 验收。

尚待完成的仍有：评价按钮/备注的专项原生交互、完整三端 UX、可靠 stream→durable 阅读目标迁移、其余 footnote/entry 能力。TekesUI #22 混合内容阅读事务与 #25 可选中富内容仍未交付；向上阅读不受追加打断、分页保持锚点及完整 Transcript/RunView 验收仍为 false。UI workspace 保持 0.0.0 / publish=false，本轮不合并或打稳定 tag。

最终原生 fixture 首次与并行 gates 同时运行时出现 busy/草稿断言失败；失败日志保留。并行 gates 结束后，相同二进制与断言隔离复测通过，系统拼音探针也通过。首次失败的原因尚未确证，不把隔离通过当成并行运行稳定性证据。
