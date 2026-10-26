# TranscriptContent / RunPort 受控思考输入（2026-10-23）

在原来源归一化及公开帧工厂上，将 None/Compact 策略接入完整 TranscriptContent、RunPort 和 ReadOnlyTranscript。复用原行规划、OutputMessage、阅读事务和 editor；不引入宿主思考状态机，不修改 TekesUI。

`set_thinking_display(TranscriptThinkingDisplayInput)` 是宿主传入的显示偏好/计时输入。先设置，再执行原 prepare/validate/publish 流程；初次 Composer 挂载仍使用原 update。revision 必须非零，单调递增；同修订不同值和旧值拒绝。启用后继续显式供应显示输入，不退回无控制模式。这里改变的是待发布显示输入；已经显示的树在成功发布前保持不变。

Transcript 在同一不可变 snapshot、自己的 scope/publication/epoch 内按 turn 生成来源帧，原 OutputMessage 在树变化之前校验。缺失来源直接失败，不以行号或父 Run 版本修补。只读 child 使用其自身 publication；无 Composer、无可写 Run。

prepared plan 捕获输入和已发布显示输入，偏好/计时改变会使旧 plan 失效；失败不会消耗行 ID 或部分替换树。发布时仍使用原 TekesUI reading token，不另存阅读位置。相同 publication 的显示更新复用历史 disclosure/editor；新 publication 即便正文相同也刷新原消息的 authority，避免仅父节点更新版本。

原 ComponentDemo 的正式 Transcript 与只读 pane 现在显式传入 Compact（revision 1、elapsed 0）；这是试验宿主默认偏好，不新增控件。第二公开 consumer 提供 prepare_thinking_run，仅转交输入和原 Run prepare。

本阶段不改变既有业务行顺序，不宣称完成 ClientKit carrier 聚合/布局、全部富文本或跨进程验收。TekesUI 固定公开 50ec49f，#53/#48 等门禁仍在，PR #10 保持 draft。前一源码的原生证据保持各自二进制身份，不能移用于本改动。

## 验证

完整 UI workspace 584 项通过、1 项原有 #49 忽略；随后仅新增一项只读 live-tick 用例，最终来源测试 4 项全部通过。实现未在完整回归后更改；全 workspace/all-target strict Clippy、格式、边界与精确 SDK 输入检查通过。根实现本阶段未变，276 项根测试保留前阶段身份。

新增覆盖原公开 SEP→RunBinding→第二 consumer→RunPort prepared plan；旧偏好计划拒绝、无部分提交、来源缺失不消耗 namespace、重复来源发布刷新消息 authority、原 editor/focus/中文 preedit 保留。只读 live-tick 从实际 Journal 投影生成，使用独立 publication 500，状态文本同 publication 刷新保留原控件，Compact 切换仍用原消息行；它是组件用例，不冒充真实 bounded-child 协议验收。

当前程序 SHA-256 `8d11baa05322c189dfc638201f759f303c46dba991ac84e60b14e172c8ae18ae` 的原生窗口通过本地 fixture 分页 20→44、锚点、向上阅读、焦点/组字、草稿隔离和重连恢复。系统拼音 `ni hao`→`你好`、组字发送/导航门禁、单次提交通过；仅软件定向按键，不是物理键盘或候选窗验收。该程序输入哈希与本阶段源码匹配，收据未借用前一程序。

基线 d8e975e CI 两项基础检查成功，两项 macOS UI 检查均失败；已读取失败 job，停于原 strict bounded_detail_reading（#53），未弱化。新提交状态以 PR Checks 为准。#48/#53 已复查仍开放，公开 SDK main 未变。

本阶段复用原共享 target、关闭 incremental；验收程序硬链接保留。新建 target 0，删除/释放 0 字节。失败的中间编译/测试记录保留；最终证据路径和摘要见[机器记录](rust-agent-ui-thinking-parent-20261023.json)。
