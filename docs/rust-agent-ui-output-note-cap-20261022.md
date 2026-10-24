# 输出脚注备注宽度（2026-10-22，实验状态）

当前公开 TekesUI 精确依赖仍为 `3bef66754a256e7723887de7d33518f5122805ad`。
AgentKit 的原 `TurnFeedback` 备注按钮按内容列宽的 40% 限宽，调用 SDK
`LabelOptions::single_line(TailEllipsis)`，保留完整 source、accessible name 和原按钮身份。
SDK 负责 shaping、ellipsis、按钮样式、命中、焦点及 IME；没有手动改写字符串或复制滚动状态机。

## 对照来源与责任

只读对照 ClientKit `TranscriptTurnFootnote.rawOutputNodes`、`TranscriptFootnoteTarget.footnoteFeedback`
和 MetalTextKit `MetalVirtualLayout.appendAside`：输出按复制、评价、备注、信息等顺序靠左；
label 类型的动作上限为完整内容列宽的 40%。Tekes `ClientAgentHostAdapters.swift` 仍提供
host 适配，不在本轮修改。其余基础样式继续使用 TekesUI token。

私有 `FeedbackRatingRow` 只持有业务控件顺序和这条 width policy，调用原 SDK
`layout_measured_children`。先按完整列测量，再在嵌套组的 intrinsic frame 中排列；
不把组宽再次乘 40%。原 rating/note 控件在重测与业务重建间提取并保留，
原 target/publication/epoch、一次性 gesture、CAS 和编辑器组字屏障不变。
备注编辑展开时仍复用既有原 SDK 文本框和表单，不另写弹层。

公开宿主要求：显示备注按钮需要 `TextSurfaceHost::shape_label`。
缺少服务由 SDK 返回 `MissingLabelServices`；不退化成多行。
没有新增公开组件或修改 endpoint/presentation API。

## 验证范围

完整 UI workspace：520 passed、1 既有 TekesUI #49 ignored；strict all-target Clippy、
demo build、fmt 和静态 UI ownership boundary 检查通过。

第二公开 consumer 使用真实 CoreText，覆盖 4 主题、1/1.4/2 字号比例、320/770/1100 列宽，
短备注与长中文/emoji/组合字符备注；另做 180 窄宽再恢复完整列。
验证单行省略、完整原文与 AX name、40% 上限、不逐次缩小、原按钮/评价组件实例，
并在更新和宽度往返期间保留另一原 Composer 的焦点、实例和完整 opaque marked baseline。
同套现有原反馈操作回归一并通过。编译诊断和最初长文本 fixture 不足以溢出的失败已保留。

冻结验收程序 `AgentKitOutputNote20261022.app`，SHA256
`4bd0a8d69233d8b64129a47c1058a086c98942c6e732767a7d096c7855e04b2f`。
同程序真实 Native0.9.3（明确 UI-only）、Pi1.3.0、DSH0.2.1-rc.2 的公开协议数据→原控件→
离屏 GPU 全部通过；覆盖原反馈控件、中文备注保存/重开、删除后重新读取、原 target
与 RunPort 路由，以及草稿保持。所有 provider/session 为隔离本地受控来源，无付费调用。
三端结果及原始日志归属见机器记录；不借用此前程序的原生或系统拼音结果。

## 仍未完成

备注限宽不等于窄列所有控件均可见。默认文字按钮的占用、原图标 #47、交互 Popover #49、
ProcessContainer #48、完整 Views 配对视觉与原生验收仍开放。
本程序没有新的原生窗口/系统拼音通过声明。前一程序在本日的复验一直 inactive、phase 0，
输入法前置条件已满足，仍需可用前台才能验证分页/reveal/焦点和系统中文组字。
PR #10 保持 draft，不合并、不打 tag。

复用既有 UI/root target；未新建 target。固定程序、来源、失败日志和验收证据保留在
`.runtime-fixtures/output-note-cap-20261022/`，摘要见[机器记录](rust-agent-ui-output-note-cap-20261022.json)。
阶段结束检查未发现 link-complete `.rcgu.o` 清理候选；共享现用缓存及证据保留，释放空间 0。
