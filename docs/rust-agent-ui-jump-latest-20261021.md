# RunView 回到最新

对应当前 ClientKit 的 `TranscriptJumpToBottomButton`、`TranscriptJumpButtonVisibilityState`、`RunUserControlsGraph` 和 `RunView.jumpToBottom`。Tekes 的 `ClientAgentHostAdapters` 仍只提供宿主导航；Jump 直接使用 presenter 跟随，不调用 record/take 历史回调。本轮只读对照两仓库，没有 Swift、Kernel 或 TekesUI 修改。

AgentKit 提供 `RunView::install_jump_to_latest(namespace)`、`RunViewIntent::JumpLatest` 和共享 `jump_run_latest`。namespace 必须独占且与 Transcript 行池、Composer、rail、context 分开。控件用原 TekesUI ButtonContent（Exact/Press、preserving_focus），继承库的样式和焦点反馈；箭头有单一“回到最新”AX名称。没有仿制旧玻璃皮肤或按钮输入状态机。

原生 demo 已安装：有真实回合且成功布局表明远离底部才显示；未知位置、近底、无回合、卸载/折叠不显示。按钮在 pane 水平居中、Composer 视觉顶部上方，regular 底部 padding 属于 Jump host，不使用文档 loose occlusion 间距代替；没有 Composer 时从 pane 底部放置。大小取 SDK IconButtonView 的公共测量值。

## 宿主接入

```rust
run.install_jump_to_latest(exclusive_namespace)?;
// ContentHost 成功 render 并发布当前帧后，正常分发原输入/AX事件。
// 同 UI turn 排空 run.take_intents(&actions)。
// 对 JumpLatest：取原 viewport 的 ReadingDocumentVersion，立即调用：
// jump_run_latest(&mut host, root, &intent, &original_version)?;
```

它先验证原 SDK reading attachment，再验证原 Run root、generation、mount/pane lifetime 和一次性凭据，最后仅调用原 `ScrollContent::follow_latest`。不触碰编辑器、网络或宿主历史，不重新附加 Transcript 阅读文档。来源/epoch更新由原 Run publication generation 退休旧操作；clone 不能再次执行。

`RunJumpVisibility` 是业务显示策略：40pt proximity、重叠 disclosure ID、incoming document 的未知状态、preserved controls insertion 的 pending inset acknowledgement。公开 `jump_visibility_mut` 供有这些能力的宿主转交明确事件；不实现 disclosure 动画、滚动或布局引擎。当前 demo 的同步 Composer 几何由原 SDK 布局采样；完整 stacked/transient UI 仍未交付，不能把策略单测当成完整 disclosure 原生验收。

## 证据与范围

- UI 全套428项、阅读/导航25项（子集）、strict all-target Clippy、fmt与边界检查通过，包含第二公开 consumer 调用。
- 新组件测试验证位置、近底隐藏、真实原按钮点击、原 reading attachment 不变、中文marked/selection/focus/editor owner不变、clone与旧 publication/SDK token拒绝。值测试覆盖未知位置、40阈值、重叠disclosure及旧 inset样本不能提前隐藏。
- 原生/GPU窗口：沿原分页按钮加载20→44项，A/B草稿与阅读独立、重连恢复；组字内完成普通Start、rail Latest，然后实际向上滚动，再点击新的Jump。Jump恢复follow并退出显示；宿主轨道历史仍恰好两条，Jump不增加历史，无提交。
- 早期用正向wheel从底部测试向上阅读而未退出follow，断言失败；当前按SDK契约使用负向delta，保留失败日志及程序。不改滚动符号、不修改SDK。

原生鼠标和 preedit 为工具合成；实体键盘、候选窗、VoiceOver、完整产品视觉仍未验收。同一验收程序也通过真实系统简体拼音软件探针：marked、发送/切换屏障、确认“你好”及显式单次提交；输入源不变。两种输入证据分别保留；三真实harness阅读证据也不由本地新Jump测试扩大范围。

证据在 `.runtime-fixtures/agentkit-jump-evidence/`，验收程序 `.runtime-fixtures/AgentKitJumpCheck.app`。继续精确消费公开 `1294987dacabb33c0e411c13ee5ea544a1123f8e`。复用现有共享target，debug0、incremental关闭；本阶段删除缓存0字节，保留程序、失败日志、固定源和收据。#42的新树持久checkpoint、rich content/selection、rail hover/preview和其余完整Views验收继续开放；不据此合并或打完整UI稳定tag。
