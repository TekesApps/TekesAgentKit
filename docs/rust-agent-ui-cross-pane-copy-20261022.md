# 跨 pane 本地复制与组字所有权

本次继续对齐 ClientKit 的原 footnote copy：控件携带原文字，由原引擎执行通用复制；`ThreadTranscriptPresenter` 不将 copy 路由为评价或执行操作。Tekes host 继续复用共享 Run 内容、承担窗口和导航。Rust demo 的组字屏障此前只放行当前 RunPort，导致另一可写 pane 或只读子历史的原复制按钮被提前拦截；只读复制路由还受 `!composing` 限制。

## 宿主接入规则

共享 `TranscriptCopyIntent` / `TranscriptCopySource`、整回合聚合和一次 `claim()` 规则不变，见[回合输出复制契约](rust-agent-ui-turn-output-copy-20261022.md)。本次只修正同进程 demo 的业务事件放行，不新增 Agent 状态机或复制 SDK 输入/焦点/布局引擎。

demo 每次事件读取当前挂载的主 RunPort、选中可写 pane，以及选中只读子历史的原 `copy_action_ids()`，并检查 SDK 可见 frame。它只在原 Run Composer 实际持有 SDK 焦点、仍在当前 focus scope 时使用这个例外。缓存、未布局、隐藏或退役 pane 不取得放行；feedback/custom editor 不继承它。原 hit、capture、焦点及事件消费继续由 TekesUI 决定。

| 原组字 owner | 复制来源 | 捕获和保护 |
|---|---|---|
| 主 Composer | 子代理 / Task / SideChat 的 RunPort | 使用该 pane 的原 scope/文字；主 editor、选区/marked baseline 和草稿不被复制覆盖 |
| 可写 pane Composer | 主 Transcript | 使用主 scope/文字；pane 的原 editor 和未确认组字保留 |
| 主 Composer | 只读有界子历史 | 使用原只读 scope/文字；不执行 endpoint read/page/refresh，不制造 Composer |

AX 只允许原复制目标的 Press，不授予 Focus 或任意部分操作。分页、刷新、会话/标签导航、发送、反馈和回合执行继续遵守各自屏障。原 SDK 的 `preserving_focus()` 保留，宿主不借点击切换编辑器，也不根据当前组字 pane 重取待复制文本。

只读本地复制在组字期间也可路由；其 endpoint read/page 路由仍保留 `!composing`。父投影失去 ready 后正文及原目标退役，恢复时不能用旧按钮 ID 激活新 pane。已捕获的复制 intent 仍只携带原文字和来源，不能变成新的会话/业务提交权限。

## 分层验证

组件/实际 demo 宿主测试覆盖可继续子代理、只读子历史、Task、SideChat 的双向 scope、原指针和 AX Press、AX Focus/会话切换/读分页阻止，以及父投影等待/恢复时旧目标拒绝。Task 使用公开 host task loader 输入与原 binding；SideChat 经公开创建/打开/发送准备一条受控 fixture 回复，复制前后 admission 数不变。测试不把该准备消息算成复制提交。

第二公开 consumer 在同一 SDK ContentHost 中挂载原 Composer 与两个独立 Transcript 子树，故意复用相同 turn/item 文本 ID，检查两份原 scope/文字、一次认领及原 editor/中文 marked baseline。它使用库垂直布局容纳内容，不宣称是产品整体布局验收。首个横向测试布局把内容排到视口外而失败；日志保留，未通过伪造 frame 或改 hit 规则修复。

原生窗口复用 `--readonly-reading-native-selftest`。设置 `TEKES_SELFTEST=text` 且使用 owned fixture 时，扩展以下闭环：

1. 原有只读分页38→75、刷新和阅读锚点；只读阶段没有 Composer。
2. 主 Composer 中文组字时，实际点击只读输出复制，读取 SDK PID 专属测试剪贴板，保持原 focus/owner/marked baseline 和阅读锚点；原分页/刷新仍被阻止。
3. 显式 Unmark 合法提交主组字，再通过原宿主 adapter 打开可继续子代理。此处允许主草稿因用户确认而改变，不能把它误报为复制污染。
4. 子 Composer 组字时点击主输出复制，私有剪贴板读回主文字；子 editor/marked baseline、主已确认草稿和零新增提交保持。

原生真实 bounded-child 模式和未启用私有测试剪贴板的原模式不执行上述扩展，收据中 `crossPaneCopyRequested` 等字段为 false；不能据基础分页通过宣称跨 pane 复制通过。真实 Task/SideChat 的窗口组合、实体键盘、候选窗、VoiceOver 和配对 Source 视觉仍需独立验收。

## 本阶段结果

496项组件/实际宿主/第二 consumer 测试、31项工具测试、strict all-target Clippy、fmt、原生构建、边界及实际 Cargo 八包241输入校验通过。新增 Task/SideChat 测试仅使用公开绑定和原 RunPort；最终构建字节与已冻结原生程序一致。

程序 `.runtime-fixtures/AgentKitCrossPaneCopy.app`，SHA256 `2705f5f75f46f945ccf5ee4debc38351439f2b2d59d8ae66d95e45446fae5375`：

- 上述 owned SEP 原生跨 pane 路径通过，私有剪贴板两次实际读回，原 editor/marked baseline 保持。最终截图已检查，记录子 Composer 组字与主输出复制；不代表只读阶段截图或配对 Source 视觉。
- 同程序原生阅读 fixture 分页20→44、前插/向上阅读、Start/Latest、A/B阅读/草稿及重连通过。
- 同程序原生计时/整回合复制回归通过：两段正式输出、结束前 live、结束后时间与 wake、原指针复制/私有剪贴板读回和原主 Composer 组字保持。
- 同程序系统简体拼音通过，前后输入源均为 `com.apple.inputmethod.SCIM.ITABC`；软件发键结果不等于实体键盘/候选窗验收。
- 同程序真实 Native Kernel0.9.3、Pi1.3.0、DSH0.2.1-rc.2 协议、历史/分页回传、混合组件至 GPU offscreen 通过。provider为隔离本地受控服务；Native 显式 UI-only，不提升为生命周期/archive通过。

原生跨 pane 本轮仅使用 owned fixture、合成预编辑及同进程 ContentHost。没有本轮真实三端原生跨 pane、Task/SideChat 原生组合或 UI 跨进程通过记录。历史三端流式/反馈/阅读以及失焦失败保持其原程序和范围，不重新归属本程序。

首个宿主测试未区分 Unmark 的合法主草稿提交而失败，已改为分别核对确认前和确认后的原草稿，不删除来源/零新增提交检查。第二 consumer 的初始不可见布局失败亦保留。没有放宽运行时代码断言来接受这两次测试错误。

原生程序和精确验证范围见[同名 JSON](rust-agent-ui-cross-pane-copy-20261022.json)。UI 保持实验状态；Sidebar/RunPort/SidePanel 的 ProcessContainer 接入仍等待 [TekesUI #48](https://github.com/TekesApps/TekesUI/issues/48)，同进程复制结果不代表 UI 跨进程验收。本次仅消费精确公开 `09b4bab44b6dfa65487250ea816b610b9df59095`，没有 SDK 或参考仓库修改，没有新基础缺口需重复提 issue。

复用既有 target，保留源代码、WIP、固定输入、失败日志、截图和验收程序；本阶段未删除缓存，释放空间0。PR #10 保持草稿，完整 View 布局、其余辅助动作、三端原生组合及最终 review/merge/tag 仍未完成。
