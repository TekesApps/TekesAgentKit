# Summary 的原编辑焦点与组字边界

后续当前状态：[子代理状态来源](rust-agent-ui-summary-child-status-20261022.md)已修正并接入原 presentation；下文将该项列为待办的是本页原程序阶段。图形附件、默认行布局和整体目标仍未完成。

ClientKit `Sources/Composer/Agent/ClientAgentSummaryView.swift` 中，`SummarySectionHeaderButton` 和 `SummaryDefaultRowView` 的 `acceptsFirstResponder` 都为 false；普通静态行的 hitTest 则返回可选择 label。Tekes 的 `ClientAgentSummaryHostAdapter.swift` 负责产品能力与内容回调，不另持一套焦点状态。

本次对齐这项已确认差异。当前精确公开 TekesUI pin 仍为 `09b4bab44b6dfa65487250ea816b610b9df59095`，不修改 SDK 或兄弟仓库。

## 公开组件契约

- Summary header 的 `Component::focusable()` 返回 false，既不因 pointer/AX Press 接管编辑焦点，也不进入 Tab 顺序或接受显式 AX Focus。唯一完整 AX name、原 CompoundButton/Disclosure、单行省略和动作仍保留。共享 section helper 使用明确的 Summary/Sidebar 策略；Sidebar 文件夹和置顶 header 保留原行为。
- 普通可执行行是 AgentKit 的 `SummaryActionRow`，`SummaryView::action_id` 仍返回原行ID。该业务组件将测量、绘制、pointer capture、取消、按键处理和 AX Press 全部转交原 `ButtonContent`，只声明不成为键盘焦点。原 SDK button 可用 `component::<SummaryActionRow>().button()` 只读检查；不再将行直接 cast 为 ButtonContent。受控发布继续保留它的原实例与 SDK activation state。
- 静态行仍使用原只读 TextFieldContent，可选择文字；usage title 同理。自定义 host Content 保持其自己的焦点、Tab、编辑与事件策略。本策略不会递归禁止 Summary 中所有控件获得焦点。

`Content::preserving_focus()` 仍保留 Tab 和显式 AX Focus 能力，不能独自表达这里的 Source first-responder 规则。本次使用 SDK 原 `Component::focusable` 契约，不实现焦点遍历、编辑、IME 或按钮状态机。

## Composer 组字期间的 disclosure

`SummaryView::is_nonfocusing_disclosure_target/at` 只识别当前原 header、有效布局和实际可见 clip；未布局、布局已失效、旧 owner/已删除ID、普通动作行均不放行。调用方还须确认 Summary 是当前已展示 pane，且组字编辑器位于独立 Run Composer，而非即将隐藏的 Summary 自定义行内。

demo 使用同一公开查询：只有原 Run Composer 是实际 focus owner、仍在 SDK 当前 focus scope、父投影 ready 且当前 tab 为 Summary，才允许原 header pointer 或无 part 的 AX Press 通过现有 IME 屏障。AX Focus、普通内容导航、其它 pane/控件、过期位置或已删除 header 不由此放行。发送与目的地切换保持原组字保护；不主动 Unmark、不提交或取消组字。真正的 domain navigation 仍由宿主负责，行本身不抢焦点并不授权在组字期间换会话。

折叠会使其原正文子树不再参与输入/AX；SDK 负责隐藏后实际失效的焦点。若宿主支持 Summary 自定义编辑器，必须先处理该编辑器的组字，不能用本查询将其隐藏。

## 验证与限制

第二公开 consumer 将真实 Composer 和 Summary 组合挂载，验证 action/header pointer、AX Press、不接受 AX Focus/Tab、原编辑器身份与 marked baseline、闭合行拒绝动作及原 scope/path 回调。另验证自定义原按钮可获得焦点并激活，静态文字仍可选择。demo 路由回归通过原 event/AX API 验证组字期间折叠/展开、拒绝 Focus 和移除后旧目标；不绕过宿主屏障。

原生专项复用 `--components --transcript-content --summary-label-native-selftest`，在原 Composer 中文 marked text 仍存在时实际 pointer 折叠、展开及原 AX facade Press，核对同一编辑器/focus/opaque baseline、唯一AX名称、Tab排除和零提交。它保留原单行标题、字号、窗口resize断言，不先 Unmark 再声称组字保持。

同名 JSON 记录精确程序、源码、测试和日志收据。预编辑与 resize 是程序注入，系统拼音软件输入法另有独立收据；都不代表实体键盘、候选窗、VoiceOver或完整配对视觉。历史标题程序与收据保持其原范围，不改写成新焦点策略的验收。

Summary 的 leading/trailing 状态附件、子代理 child-session 状态来源、hover/divider 和完整默认行/产品行布局仍未完成；图标/不定进度原 Content 入口仍跟踪 TekesUI #47。其它 Views、真实三 harness 未完项和 Native lifecycle/archive 验收仍属整体目标。PR #10 保持实验草稿，不据此合并或打稳定 tag。

本地业务实现469项UI tests（含第二consumer）、31项脚本 tests、边界及8包241输入闭包核验通过。最终原生脚本的下一帧坐标修正另经严格all-target Clippy/build和以上同一程序原生回归；最终commit仍需CI覆盖完整树。最终程序SHA256：`1d2170f9f82973058a3f62b826f3c7bf427fc254094ebb6fd2c5a39c9b129d39`。首个程序在恢复字号后使用上一帧坐标，原生点击未命中，故保留为未验收中间程序；修正仅等待新布局后读取原控件位置，未削弱任何焦点/marked断言。

复用原共享target，debug0、incremental关闭，未新建独立缓存；保留失败与最终程序、固定依赖和收据，本阶段无确认可删除的专用target，删除0字节。
