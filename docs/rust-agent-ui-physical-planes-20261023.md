# Transcript 实际分区与普通回合 Activity 折叠

基于 `c04efc6`，将已有共享 plane/presentation 判定接入实际 Content 树。精确 TekesUI 依赖保持公开 `50ec49f3446924fdbc66b3cff22cd8cfe150fe7c`；UI 仍是未发布的 0.0.0 实验包。

## 布局与节点所有权

同一消息的原 reasoning、工具子项和正文现在分属独立阅读行。reasoning/工具留在 Activity，只有规范化来源明确标为 final、已提交的 assistant 正文移到整段 Activity 之后；live candidate 不提前结算。输入和输入 footnote 仍由原 InputMessage 右对齐，机器内容沿用原 SDK 的左对齐和 tokens。

原 Message 根保留正文和唯一消息控制器。`OutputMessageFragment` 分别持有原 reasoning 与工具 Content，每个实际节点只有一个所有者。更新前借用完整节点集合校验；应用时合回原节点，调用原消息更新/复用算法，再拆回对应行。空正文控制器保留在隐藏存储中，不增加空阅读行或间距。折叠也保留实际节点，不复制 editor 或另建消息状态机。

完整校验仍先于树修改和 namespace 分配，包括 scope、来源/frame、publication/epoch、消息/receipt 身份、文本版本、工具 header revision 和 namespace 上限。新增 fragment owner/scope/完整性检查；消息控制器也必须属于当前 Transcript scope。跨 scope graft 的反例会拒绝整个计划，恢复原控件后原计划仍有效。

普通已结算回合显示原 TekesUI ButtonContent：completed 默认折叠，异常终止默认展开；无 Activity 时只显示被动 Summary。Working、settled disclosure、settled Summary 使用不同 row key，旧 live 状态节点退休。默认值来自共享 presentation，用户展开选择由 Kit 按本视图 scope/turn 保留。工具自己的详情/连续分组展开选择也保留。final Output 移走后，连续的独立工具重新按原 ToolRun 规则分组。

## 宿主 API 与事件

- 新增 `TranscriptContentRowKey::MessageFragment(identity, kind)`；kind 为 `Reasoning` / `Tools`，ID 在原消息 namespace 内派生，不使用消息文本或列表偏移作身份。
- `ActivityDisclosure(turn)` 表示交互式历史状态，`ActivitySummary(turn)` 表示被动状态；原 `ActivityStatus(turn)` 只表示 live Working。
- `TranscriptContent::message_content_ids(identity)` 可查询正文及保留的 Activity 原 part ID；`row_id` / prepared-plan 的阅读列表只包含实际挂载的行。part ID 不是 durable session pointer 或动作权限。
- 独立 OutputMessage 的公开入口保持；在 Transcript 内，原 Message 根只含正文。查询 reasoning/工具应使用新 fragment 行的 `OutputMessageFragment` 或上面的整体 part 查询；不要直接更新已拆分的子 Message，整体更新必须经过 Transcript / RunPort。
- `TranscriptContentLabels.activity_settled: fn(&TranscriptTurn) -> String` 是新的宿主 copy hook，完整结构字面量需补字段或使用 Default update。默认文字与 ClientKit 的终止动词及原 elapsed formatter 对齐，没有本地颜色或字体主题。
- `RunPort::set_activity_expanded`、`ReadOnlyTranscript::set_activity_expanded`、低层 Transcript 的同名方法只改变 Kit 本地选择。只读路径使用自己的历史 publication，不构造可写 Run。
- `RunPortIntent::TranscriptLayoutChanged { scope }` 通知宿主用同一捕获快照重新发布；不要为这个事件读取 endpoint、提交、取消或重试。穷举匹配需增加这个分支。

所有树变化继续通过原 prepared-plan 和 TekesUI reading transaction。local activity generation 会使旧计划失效，即使 provider publication 没变。reading display revision 应从当前 SDK token 递增，与 provider publication 分开；测试 consumer 的旧 helper 也已修正这一点。

组字期间可放行 Kit `activity_action_ids()` 返回的当前挂载、焦点保持的本地控件，以及原复制控件。宿主仅转交原 SDK 事件；Kit 在发送/分页/反馈门禁之前处理这些本地选择，其他 mutation 门禁保留。独立 Transcript / 只读宿主可调用 `handle_activity_actions` 判断是否需要原阅读发布。没有复制焦点、IME、滚动或渲染实现。

## 范围与剩余项

本阶段落实普通回合的实际分区和整段 disclosure，**不是完整 ClientKit Views 对齐或完整 UI 交付**。`BetweenInputs` 目前保留已知时间线，ClientKit 的回合内追加输入分段折叠仍待补；没有 prompt role 的 initial System 继续 Direct，避免猜测和隐藏内容。narration carrier/fallback 的剩余来源差异、富内容/选择、glyph/disclosure chrome、最终配对布局和完整 AX 验收仍开放。

TekesUI #48 的正式交互式 ProcessContainer 同/跨进程、#53 的严格嵌套阅读等上游门槛未解决。标准详情仍用原 inline 路径；未放宽严格 CI，未复制基础能力。已发布 AgentKit 仍为 v0.3.8，本分支增量需未来不可变发布 pin。

最终 UI workspace **596 项通过、1 项既有 #49 忽略**；全 targets 严格 Clippy、格式和 Rust 边界检查通过。根源码/manifest/lock 未变，根 289 项与根 Clippy 沿用此前独立证据。

当前 UI 冻结程序为 `c90c9f3b9baf473e52ac1949789eff760ca636ddfcd588029f885eaf461b58cf`。owned 协议原生窗口实际点击初次展开、组字期间折叠/再展开；原工具和推理文本控件实例、阅读行/相对位置、编辑器实例、焦点及 marked baseline 均保留，resize/字号重排仍通过且无提交。该专项 realHarness=false、physicalIme=false。

同一程序搭配未变根源码的固定 headless，在真实 Pi 1.3.0、DSH 0.2.1-rc.2、Native Kernel 0.9.3 上各通过原生 stream、stream-reading、public-page 三个 profile；两条三端 driver 退出 0。本地合成 HTTP provider 不代表真实外部模型。stream-reading 的 preedit/wheel 为 synthetic；public page 使用公开 API，Pi 50→56、DSH 50→59、Native 50→56，nativePagingControl=false。所有未验 false 字段保留，mounted offscreen completeRunPort/completeTranscript/richContent/keyedReadingAcceptance 仍为 false。

该程序另外通过一次正常 LaunchServices 系统拼音验证：原输入源、系统组字、发送/导航屏障、Commit 和单次提交。它使用 PID 定向软件按键，不验证物理键盘、候选窗或 VoiceOver；#41 既有首键失败仍保留，稳定性未证明。

c04efc6 的基础 CI 两项成功，UI 两项失败。日志归档下载均遇到 EOF，随后由官方 job API 核实失败 step 为严格 nested-detail reading；不把未取得的日志当作本轮根因收据。既有 #53 仍开放，PR #10 保持 draft，未合并或打 tag。

验证、精确程序及失败/成功日志摘要见 [机器记录](rust-agent-ui-physical-planes-20261023.json)。组件、第二 consumer、owned fixture 原生窗口、真实 harness 协议/原生，以及系统软件 IME 分开记录；不转移旧程序的收据，也不把 synthetic preedit 当作系统拼音。

复用既有 `/tmp/agentkit-layout-text-292-target`、关闭 incremental。保留源码/WIP、固定依赖、旧失败记录、冻结程序和验收证据，未新建 target 或清理共享缓存。
