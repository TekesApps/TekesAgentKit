# 三真实 Harness：流式追加期间向上阅读

当前公开精确 TekesUI 仍是 `1294987dacabb33c0e411c13ee5ea544a1123f8e`。这次补原生验收覆盖，生产 Run/Transcript/Composer 和 presentation 状态机未改；不以新探针替代同名组件，不复制 SDK 编辑或滚动逻辑。

## 场景及判据

新 opt-in 模式 `--stream-reading-native-selftest` 要求显式 owned endpoint、正式 Transcript 窗口和已播种历史，不支持 offscreen。runner 的 `--ui-native-stream-reading` 要与 `--ui-components --ui-transcript-content --ui-history-pages` 同用。

每个 harness 经其公开 prompt/model API 先播种28个已完成回合；两个会话的草稿必须为空，探针才设置本次独立草稿。通过原 SDK 显式到最新、原生 Send 按钮发送一次 fixture 输入，收到真实 active preview 后向上滚动。锚点必须属于**先前历史回合**，并明确不属于当前输入/回复。

在源仍流式运行时，保留原 Composer 并注入中文 preedit（选区2..4）。原 endpoint 持续回传两段长中文及结束标志；只把 provisional=true 期间新增长度计为流内追加，不以最终结算充数。每次回调检查：

- 原 SDK reading item、item相对真实viewport的局部位置保持（1逻辑点容差），且未恢复sticky follow。
- 原编辑器实例、焦点、opaque baseline及marked text保持；不以新editor或焦点reveal“修复”。
- 完整结算文本逐字匹配，原显式输入恰好一条；不把历史28个输入误算成本次提交。
- Unmark后中文草稿正常提交到原draft；重连后同一阅读位置/文本仍在且不重发。
- 切换B保留其独立草稿，返回A仍保留A草稿、原reader和editor owner；不把unmount前opaque baseline当持久草稿。

Native0.9.1、Pi1.3.0、DSH0.2.1-rc.2均使用同一冻结程序和共享组件。模型服务只在loopback提供受控流，凭证及会话使用隔离owned状态；不是付费模型调用或私有数据库重放。

## 本轮证据

三端均观察到3次流内追加，上述原生窗口/阅读/组字/重连/隔离断言通过。UI workspace432项（含第二公开API consumer）、脚本26项、strict all-target Clippy、fmt与依赖边界通过。源码、同一程序SHA及逐harness收据分别保存，不把offscreen历史页JSON与原生结果合并计数。

截图为原生窗口在完成结算、仍向上阅读时的采集。已检查Native截图，旧历史仍在viewport；它不是与ClientKit同内容、同窗口、同主题的配对视觉验收。工具合成滚轮、鼠标和AppKit preedit；不提升实体键盘/IME候选窗、VoiceOver、工具展开后阅读、所有动画/主题/窗口尺寸或性能结论。此前普通流式选区/回合控件、分页、只读bounded pane等证据保持各自范围。

证据在 `.runtime-fixtures/agentkit-stream-reading-evidence/`：`final-three.log`、三份 `final*.native-stream-reading.log`及原生PNG、程序/源码哈希、UI与Clippy日志。补充旧回合来源断言之前的成功程序/日志另存 `before-explicit-old-turn-assertion/`；早期编译时误启动旧程序的无效run已停止并保留日志，不计验收。

继续复用现有 `/tmp/agentkit-layout-text-292-target` 和root target，debug0、incremental关闭；无新target、删除缓存0字节。完整UI、rich content/selection、rail hover、SidePanel/Sidebar其余显示对照、持久checkpoint(#42)及最终review/合并/tag仍未完成。

## 后续公开 SDK

已通过GitHub核对TekesUI #43合入及 annotated tag `remediation-toolbar-checkpoint-20261021`，dereference为 `09b4bab44b6dfa65487250ea816b610b9df59095`。本轮验收没有切换live pin；新navbar/toolbar/ProcessContainer能力须再用隔离公开consumer独立接收，不能从活动工作树替换当前冻结输入。新增ToolbarLabel的源码兼容性及remote View键盘/IME/AX边界需逐项核对。
