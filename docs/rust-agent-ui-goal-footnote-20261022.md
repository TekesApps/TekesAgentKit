# 目标终态脚注与首次 Transcript 安装（2026-10-22）

这次在现有 presentation → RunBinding → RunPort → Transcript 路径补目标终态显示，
没有新增 RPC、轮询、目标驱动器或 UI 状态机。TekesUI 仍精确固定为公开
`50ec49f3446924fdbc66b3cff22cd8cfe150fe7c`。AgentKit 的已发布基线仍为 v0.3.8，
新增共享 API 是仓库内开发增量，尚未发布；不能称为未修改的 v0.3.8 公共 API。

## 与当前 ClientKit 的对应

只读对照 `ClientSQLiteSessionStore` 的 goal projection、`ThreadTranscriptPresenter.goalCompletionDuration`、
`TranscriptMetalSnapshotBuilder.goalTerminalTurnID`、`TranscriptTurnFootnote` 和 Tekes 的共享组件接入。
当前 ClientKit SEP 投影的 source/completed turn ID 都为 nil，目标累计耗时为零；
终态状态采用最后一个已有最终回答的 canonical turn 作为显示回退位置。
这是终态标签的位置规则，不证明该 turn 的输入创建了目标。

Rust 对 paused、blocked、complete 使用同一终态集合，按 canonical turn 顺序查找可见、
非 provisional 的最终 assistant 项。新的输入、工具、隐藏回答和流式预览不会冒充最终回答。
active 与未来未知 phase 不显示终态标签；未知 wire 字符串原样保留，不改写为本地别名。
没有明确来源时，不添加“作为目标发送”标记，不显示或估算目标耗时。

## 公开 API 与来源

- presentation 的 `GoalFacts` 从原 `SessionProjection.projections["goal"]` 读取 scope、
  原 projection sequence、goal ID/revision/phase。sequence 是原投影输入的 cut，不能用来猜 turn 身份。
  支持 Pi 的平铺 GoalView 与 DSH 的 `{ goal }` fold；与 ClientKit 一样，读取先检查 objective 字符串和来源 createdAt。null 是无目标，格式错误产生诊断。
- `RunSnapshot.goal` 与 Transcript、connection epoch 在同一 authority 读取中发布。
  老序列化快照缺少此字段时默认为 None；失败的整体 refresh 不把旧 goal 当作当前事实。
  RunBinding 继续传递这份原 RunSnapshot，不增加一个 goal store。
- `TranscriptFootnoteTiming.goal` 可供独立 Transcript 宿主消费；`for_run` 从同一 Run 帧导出，
  仅在原 scope 和新鲜连接/历史一致时展示。RunPort 自动使用它，无需宿主另接目标状态机。
- `TurnFootnoteDisplay.goal_state` 是终态读出；`TurnFootnoteLabels.goal_state` 是宿主本地化回调，
  `TurnFootnote.goal_state_id()` 提供原只读标签 ID。UI 包重新导出共享 `GoalFacts/GoalPhase`，不复制类型。
- scope 或身份错误的 Run 输入在修改 Composer 前被整帧拒绝；直接 Transcript 输入也检查 goal scope。
  goal 变化参与已有 publication/整帧计划校验。它不赋予任何 mutation 能力。

输出标签保留在机器内容的左侧 aside 内，沿用原 SDK TextContent、caption 角色和单行布局。
现有标签节点可跨同源状态更新保留；没有覆盖基础样式、绘制目标图标或实现自己的文本/滚动引擎。
目标图标仍属于已跟踪的 TekesUI #47，当前提供文字读出，不宣称完整图标视觉对齐。
有界只读子历史没有自行推断父目标状态。

## 首次挂载修复

新离屏验证暴露了 late-install 缺口：先发布 Run、后安装 Transcript 时，
`RunPort.install_transcript` 原来只传基础 Transcript，漏掉已有 goal 与显式宿主时钟。
现在由 RunPort 使用完整的同帧 footnote 输入初始化 Transcript；demo 移除安装前的重复局部发布。
新增回归覆盖 late install，正常更新则检查原 footer/editor、焦点和中文 marked baseline 保留。

## 证据范围与失败保留

`--components --transcript-content --panel-navigation-toolbar --goal-footnote-fixture --offscreen <png>`
提供隔离的公开 control projection，不调用真实模型服务、不操作桌面。
探针通过原 Kit 回到最新回调和 SDK reading transaction 把目标标签放入视口，
最终图片与收据证明目标状态标签位于原 footer 的可见视口中，不是另画的样例行。
窄列沿用 SDK 单行省略，收据/原节点保留完整 `目标 complete`；不宣称所有窄宽下完整文本均可见。

初次 late-install 失败、修复后的节点读出、过早捕获阅读手势被 fence 拒绝的失败，以及最终可见标签
使用独立日志/图片/程序保留。最终专项样例补齐公开 GoalView/fold 字段并重新验证；此前不完整样例的结果不计作最终契约通过，收紧校验后暴露的一处旧测试样例失败也保留。探针在首轮布局发布宿主导航 context 后，再安排该 context，
随后只捕获并派发一次新原手势；没有重试旧手势、绕过 lifetime 检查或抢回焦点。
这属于同进程离屏交互/GPU 证据，不是原生窗口、实体输入、候选窗、VoiceOver 或跨进程验收。

Native 0.9.3、Pi 1.3.0、DSH 0.2.1-rc.2 的正常协议→原组件树→离屏 GPU 回归另行保留。
三端本轮样例的 goal 专用 proof 均为空，不能作为三端真实目标生命周期或终态显示通过；
Native 仍是明确 UI-only，不包含生命周期/有界 child 验收。原生 SideChat 菜单探针仍待桌面协调。

最终本地验证：共享 workspace 193 项通过；UI workspace 537 项通过、1 项既有 TekesUI #49 忽略；
root/UI 严格 all-targets Clippy、fmt 与边界检查通过。Cargo 实际输入为原固定公开 8 包/243 文件。

[机器收据](rust-agent-ui-goal-footnote-20261022.json) 记录最终程序、源码输入、各项测试、原收据和证据哈希。
原始证据位于 `.runtime-fixtures/goal-footnote-20261022/`；不同程序的结果不相互转移。
复用现有共享 target，未创建独立构建缓存；保留六份不同程序的验收/失败证据，目录约 415 MiB；删除/释放为 0。
PR #10 保持 draft，未合并或打 tag。完整目标、ProcessContainer #48、窗口级接入及其余 Views 对齐仍未完成。
