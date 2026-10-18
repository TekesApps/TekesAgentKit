# TekesUI 开发任务：AgentKit 基础 Run 接入

日期：2026-10-18。接收方：TekesUI；消费方：TekesAgentKit。以下七项可分别建任务，保留 TKUI ID 便于联调。

## 范围与基准

消费侧已完成连接状态、纯文本 Transcript、基础 Composer、发送/取消、流式回传、草稿切换、重连及历史分页数据闭环，复用既有 presentation。本文要求补齐通用 UI 能力，不要求 TekesUI 实现 Agent、Session 或 harness 协议。

基准为 AgentKit `ui/tekesui-source.json`：TekesUI base commit `bc3de79466834e99062a1b4ef0872323e2860001`，源码清单 SHA-256 `1a56297ad59d17f17a955d3d22ca85776ac96fe656629e19367309a7e55f12f0`。它包含该 commit 之外的开发文件，**不代表 TekesUI 当前所有分支仍缺这些能力**。接手时先核对最新公开实现；已有等价接口则补文档/测试并给出消费方式，不重复开发。

接口名称以下均为建议，最终名称由 TekesUI 决定；事件信息、行为约束和验收要求是交付契约。源码路径均相对 TekesUI 根目录。历史缺口摘要见 [tekesui-gaps.md](tekesui-gaps.md)，消费边界见 [接入契约](tekesui-integration-contract.md)。

## 优先级与实施顺序

| 顺序 | ID | 优先级 | 交付结果 | 消费侧解除的限制 |
|---|---|---|---|---|
| 1 | TKUI-001 | P0：生产阻塞 | 可跨线程唤醒 UI 的公开代理 | 移除持续 `--frame-pump` |
| 2 | TKUI-002 | P0：生产阻塞 | 历史前插后的稳定阅读锚点 | 分页不打断向上阅读 |
| 3 | TKUI-003 | P1：完整 Composer 必需 | 文本命令、修饰键及 IME 路由契约 | 安全支持多行与键盘提交策略 |
| 4 | TKUI-005 | P1：草稿同步可靠性 | 外部值更新事务与组字冲突处理 | 不直接替换 TextInput，同步不丢选区/undo |
| 5 | TKUI-006 | P1：基础文本选择 | 指针修饰键、按钮与点击次数透传 | Shift-click、双击/三击选择 |
| 6 | TKUI-007 | P1：完整 Composer 必需 | 可约束的多行/填充文本视口 | 组合层直接配置 Transcript/Composer 高度 |
| 7 | TKUI-004 | P2：复杂组合前完成 | 带目标和来源的动作事件 | 不再轮询 Button activation 计数 |

P0 两项可独立推进。TKUI-003 与 TKUI-007 共同完成多行 Composer；TKUI-005 与 TKUI-003 共用组字生命周期语义。TKUI-004 不阻塞当前基础原型。

## TKUI-001 · P0 · 后台事件唤醒与窗口生命周期

**问题与定位**：`crates/tekes-ui-platform-macos/src/shared.rs` 的 Window 提供 UI 线程 `request_redraw` / `set_animating`，缺少可交给异步任务的公开唤醒代理。消费侧目前显式持续请求帧，才能发现网络状态变化。

**需要的 API / 事件**：

- 从 UI 线程获取 `Send + Clone` 的 `WindowWake` 或 `EventLoopProxy`；Window 本身不必变成 Send。
- 后台 `wake()` 将请求投递到所属 UI 事件循环，产生可识别的 wake 事件/回调，随后由宿主读取最新业务 snapshot、标脏并请求绘制。
- 明确返回值/关闭语义，例如已排队、已关闭；克隆代理不延长窗口存活。允许合并请求，但最后一次变更不能因合并竞态被遗漏。
- 不要求后台闭包持有 UI 对象，不要求 TekesUI 接入 Tokio；唤醒只传递通知，不携带 Agent 数据。

**验收标准**：

- 禁用 `set_animating(true)`，窗口静止时，工作线程更新共享数据并 wake；无需鼠标/键盘输入，下一次事件循环处理即显示新状态。
- 多线程突发请求可合并，最终数据必可见；覆盖“UI 清除 pending 的同时后台再次请求”竞态，不能丢失最后通知。
- 窗口关闭后继续调用所有代理不会 panic、访问已释放对象或重新创建窗口；代理只唤醒目标窗口/宿主。
- 提供事件/帧计数证据：稳定无输入、无动画、无后台变更时，不存在用于轮询数据的周期性唤醒。正常系统重绘不算失败，不要求机器 CPU 绝对为零。
- AgentKit demo 移除持续 frame pump 后，流式文本、取消结果、连接状态变化均能主动显示。

## TKUI-002 · P0 · 历史前插阅读锚点

**问题与定位**：`crates/tekes-ui-controls/src/reading.rs` 的 ReadingController 已支持尾追加和宽度/文字缩放重排；非尾追加按新文档处理，Earlier 前插后无法定位原来正在阅读的内容。

**需要的 API / 属性**：

- 公开 capture/restore 或等价原子更新接口。锚点至少能表达稳定 item key、item 内 UTF-16 位置及其相对视口的纵向偏移。
- 若使用纯文本 TextSurface，允许消费方提供旧/新区间映射或 prepend delta，达到等价效果；不得要求消费方自行计算像素滚动补偿。
- 输入包含文档身份/版本，防止将 A 的锚点恢复到 B。明确锚点项目删除、截断或映射失败时的返回结果及确定性 fallback。
- 维持 reading/follow-latest 意图，与选区、焦点、组字分离。锚点信息是通用文本/列表能力，不含 Session ID 类型。

**验收标准**：

- 在长文档中段向上阅读，前插一页后，同一 item 的同一文本位置仍在原视口位置；固定字体/宽度下误差不超过 1 个逻辑像素。
- 连续两次前插、前插与尾追加同批发生均保持阅读位置，不跳到底部；普通尾追加与显式“回到最新”的既有行为不回归。
- 前插同时改变宽度/文字缩放时保留逻辑文本位置；重排后维持该位置可见，遇边界允许合法 clamp。
- A→B→A、锚点项被删除、旧文档版本回调均有测试；不能套用其他文档锚点或恢复到过期偏移。
- 不抢 Composer 焦点；已有选区按明确的文本映射策略保留或显式报告失效，不静默选择另一段文字。
- 用真实布局结果验收并提供原生窗口分页演示；数据页正确不等于锚点验收通过。

## TKUI-003 · P1 · 文本命令、修饰键与 IME 消费语义

**问题与定位**：`composition.rs` / `composition/adapters.rs` 的 ContentEvent::Key 缺少完整修饰键和文本服务路由信息；TextFieldContent 未处理标准换行命令。ContentHost **已有** Tab/Backtab 焦点导航 fallback，本项要求明确编辑模式及组字期间的安全语义，不是新增另一套焦点系统。

**需要的 API / 事件**：

- 文本事件携带或可查询可靠的 text-services disposition：已由 IME 消费、已处理编辑命令、未处理键等。不能让宿主猜测同一物理键的 Key/TextCommand/Commit 是否应再次触发动作。
- Key 事件保留 Shift/Command/Option/Control、按下/抬起、repeat 信息；提交意图只从未被编辑/IME 消费的动作产生。
- 编辑模式显式区分单行/多行；多行正确处理 `insertNewline:` / `insertLineBreak:`。Tab 明确配置为焦点导航或插入缩进；Backtab 对应反向导航或既定编辑策略。
- 给宿主配置或回调入口以选择 Enter/Command-Enter 等提交策略。通用控件只发出提交意图，是否发送、发到哪个 destination 由 AgentKit 决定。
- 文档说明 marked text、Commit、Unmark、Cancel 的先后关系，以及控件/焦点路由如何处理组字期间的 Tab、Escape、Enter。

**验收标准**：

- 合成事件覆盖单行、多行、修饰键和重复键；同一个原生按键链最多产生一次编辑或提交结果，不双换行、不双提交。
- 系统中文拼音输入法现场验证：候选确认 Enter 仅确认组字；组字期间 Tab/Escape 按文本服务语义处理，不意外跳焦点、发送或关闭窗口。
- 非组字下，配置的换行/提交快捷键按约定工作；不配置提交策略时不产生业务发送动作。
- 既有 Tab/Shift-Tab 焦点导航保留，插入 Tab 模式不被 host fallback 提前截获；通过公开 API 的独立 consumer 可重现。
- 原生物理 IME 结果单独记录，合成事件通过不能替代它。

## TKUI-005 · P1 · 受控编辑值同步事务

**问题与定位**：TextInput 的公开字段可改，但消费侧无法以正式事务表达“确认发送后清空”“恢复草稿”“同版本重复刷新”等不同意图；直接构造新 TextInput 会重置原生编辑状态。

**需要的 API / 属性**：

- `apply_external_value` 或等价接口，输入文档 key、单调 revision/预期基线、完整文本或 edit delta；身份由消费方传入，TekesUI 不理解 Session。
- 显式 selection 策略（保留并映射 / 指定 / 重置）和 undo 策略（保持 / 新事务 / 新文档重置）；不能因每次同步而隐式清空 undo。
- 明确 composition 冲突策略：默认 defer/reject，不隐式提交或取消 preedit。若支持显式强制替换，必须单独 opt-in 并说明效果。
- 返回 `Applied / Unchanged / Deferred / Conflict` 等可区分结果。若内部排队，须有最终应用通知和版本检查；不能在组字结束后静默覆盖期间新产生的本地输入。

**验收标准**：

- 同 key/revision/value 重复同步无变化，选区、caret、undo 和 preedit 均不被重置。
- 同文档的合法增量按策略映射选区；确认发送后的清空形成明确编辑事务；新文档重置不继承上一文档 undo。
- preedit 期间外部值按策略 defer/reject；之后发生本地编辑或更高 revision，旧请求不能覆盖新内容，必须返回/通知冲突或失效。
- A→B→A 草稿与编辑器实例隔离；晚到 A 的回填不能修改 B，也不能意外恢复已发送的旧文本。
- 中文、emoji、组合字符的 UTF-16 选区映射不产生非法边界；文档注明 undo 后能否恢复清空前内容及原因。

## TKUI-006 · P1 · 指针选择元数据透传

**问题与定位**：平台 event.rs 已有 modifiers、button、click_count 信息，组合层 PointerDown 只保留位置；TextFieldContent 将 `pointer_down` 的 shift/click count 固定为 false/1。要求补齐公共事件链，不重写已有 TextSurface 选择算法。

**需要的 API / 事件**：

- PointerDown/Up 等必要事件包含位置、button、modifiers、click count；拖动保留按钮/修饰键及现有 capture/cancel 语义。
- macOS → composition → TextFieldContent/TextSurface 全链无损传递。现有简化事件的兼容/迁移方式写清楚。
- 主按钮选择、其他按钮上下文操作、控件激活的规则明确；次按钮不得被解释为发送/普通按钮激活。

**验收标准**：

- 单击插入 caret；Shift-click 从原 anchor 扩展选区；双击/三击遵循 TextSurface 定义并文档化的词/段落选择规则。
- 中文、emoji、组合字符选择保持合法边界；拖选超出视口时沿用原有 capture/自动滚动行为。
- 次按钮点击不移动 caret、不提交、不激活普通按钮（除非组件显式定义该操作）；窗口失焦/取消后无残留拖选。
- 平台事件转译测试验证字段不丢失，原生窗口实际 Shift-click、双击、三击单独验收。

## TKUI-007 · P1 · 可配置文本视口与多行尺寸约束

**问题与定位**：TextFieldContent 的 measure/arrange 经固定表单 slot，无法直接表达填满剩余空间的 Transcript 或有最大高度的多行 Composer。目前消费侧使用公开 TextSurface 自行组合阅读视口；不要求新造文本渲染器。

**需要的属性 / API**：

- 文本视口尺寸策略：固定高度、填充可用空间、按内容增长并限制 min/max 高度，或等价的布局约束组合。
- 单行/多行、wrap、read-only、内部滚动策略可配置；label/help/error 占用与文本 viewport 分开计算。
- 属性统一驱动测量、排布、绘制裁剪、命中测试、caret/IME 候选位置及 AX frame。超过上限使用现有 TextSurface 滚动，不做第二套滚动状态。

**验收标准**：

- 纵向 Stack 中 Transcript 填满余量；Composer 在最小/最大高度间随内容增长，达到上限后内部滚动，发送按钮仍可见。
- 宽度变化、中英混排、长行、换行、文字缩放下无文字溢出/裁剪错位；read-only 仍可选择复制但不能编辑。
- 非零窗口原点、祖先 clip、不同 backing scale 下命中位置、caret 与候选窗 rect、AX frame 与最终 viewport 一致。
- 改变视口高度不丢失组字/选区/undo，不抢焦点；Transcript 继续遵循 TKUI-002 阅读锚点。
- 原有单行表单的默认尺寸和布局不回归；公开组合 API consumer 无需重写 Component::arrange 即可配置上述两种文本视口。

## TKUI-004 · P2 · 带目标与来源的控件动作

**问题与定位**：ContentResponse 聚合 activation；目前消费方读取 ButtonContent.activations 计数判断哪个按钮触发。复杂组合需要稳定的类型化事件，不能依赖逐帧轮询计数。

**需要的 API / 事件**：

- 公开动作队列/返回值/callback，事件至少包含 ComponentId、动作 kind、触发来源（pointer / keyboard / accessibility）。
- 明确动作顺序、一次投递语义、drain 生命周期；焦点变化/组件重挂载不能回放旧动作。
- 与既有 consumed/changed 分开表达；公共按钮、键盘激活与 AX action 使用同一动作出口。
- TekesUI 仅携带通用 ID/动作；AgentKit 在处理该次事件时捕获所展示的 destination/scope，不让上游承担 Agent 路由。

**验收标准**：

- 指针点击、Space/Enter、AX press 各自产生且只产生一次带正确 ID/source 的激活；disabled 控件不产生动作。
- 嵌套组件与多动作事件保持定义顺序；空 drain 不重复通知，移除/重建同 ID 控件不重放旧激活。
- handler 中触发重绘/结构变更时无重入借用崩溃；文档明确允许的回调行为，必要时使用事件队列交付。
- AgentKit 可移除 activations 计数轮询；A 画面产生的发送/取消/分页意图在切换 B 后仍携带原 A scope，且不被新选择重定向。

## 交付与联合验收

每项交付应包含公开 Rust API、行为文档、确定性回归、公开 API 示例及变更说明；最终提供可公开获取的不可变 commit/tag。开发工作树路径和仅 base commit 不足以成为发布 pin。

TekesUI 负责通用事件、控件、布局、文本/滚动语义和平台桥接；AgentKit 负责消费接口并移除临时消费方式，保留既有 presentation、destination 隔离及提交身份。本文不授权修改 TekesUI；由那边实施，当前仓库仅提供需求与验收案例。

关闭任务时分别给出证据：

1. TekesUI 自身测试与一个不含 Agent 状态的公开 API consumer。
2. AgentKit UI workspace、demo host、第二 consumer 通过格式/Clippy/构建/相关回归；默认 headless 构建不受影响。
3. 原生交互记录：物理中文 IME、鼠标选择和 VoiceOver 分开列通过/未测；不能由合成输入或离屏 PNG 替代。
4. TKUI-001 的 idle/wake 计数与 TKUI-002 的真实布局锚点数据；必要的原生体验验证单独记录。

P0 关闭后可验收无持续 frame pump 的基础 Run；P1 完成后再验收完整 Composer 与稳定草稿编辑体验。任务、子代理、SideChat、Summary、富文本工具卡片、虚拟化与大历史性能仍是后续范围，不随这七项自动宣称完成。
