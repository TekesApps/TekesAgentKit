# 工具卡片事实与终端退出状态

本轮对照 ClientKit `TranscriptToolCardModel.swift` 的 derive 分支及
`SessionEventProjector.swift` → `TranscriptMetalAppBridges.swift` → `TranscriptMetalActivity.swift`
的实际桥接，补充既有 `TranscriptToolCall` 的共享只读派生。
Tekes 宿主仍通过 `ClientRunViewController` 消费 ClientKit，未修改 Swift、Kernel 或 TekesUI。

## 共享 API 与原组件

- `presentation_card()` 返回 `Option<TranscriptToolCardModel>`，公开 terminal、search、web、read、diff 数据变体。
  这是 ClientKit 的工具族/参数/meta 兼容派生，不是完整的 producer render-intent renderer。
  未知工具、无效字段或不能确认的形状返回 None；原 message、meta/error、call/result view 和位置保留。
- `presentation_result_text()` 集中读取公开文本 content，空结果可使用结构化错误的 name/code。
  不创建媒体引用、不读取文件或 harness 内部数据；图片/音频仍保留原消息，未宣称媒体呈现完成。
- UI 组件入口重新导出卡片与 row 类型，第二 consumer 不需访问私有模块或新增内部依赖。
  卡片完整内容只应在展开时派生；折叠行仅读取终端退出标记，不构造其它隐藏卡片。
- 原 OutputMessage/ToolActivity 继续使用原 disclosure、详情 text owner 与 SDK 单行 label；
  默认 phase 文本使用共享 row 的显示状态。没有新增业务状态机、字形/选区算法或基础控件副本。

## 纠正投影与 View 桥接规则

上一轮对“exit-status-only”的描述过宽。当前 ClientKit 在公开 meta/result-view 检出 exit/signal/timeout 后，
把它并入 `failed`；此时共享行使用结果错误摘要，卡片回退到通用 disclosure。
只有 `phase=done` 且 `failed=false`、由旧 shell 正文末尾退出标记识别的显示层失败，才保留动作摘要。
该判定不依赖参数是否合法，也不要求 persistent shell 已具备可展开卡片。

本轮先复现：旧 shell 的 `[exit code: 2]` 被显示为 ok。
进一步追桥接后又以回归复现 meta 失败未走通用详情、错误摘要与 persistent shell 的边界差异，原失败日志保留。
现在不改变原 canonical phase、结果和父回合结算，仅修正显示派生与 generic-card fallback。
旧版 row 记录保留其历史验证范围，并在入口注明本次校正。

终端卡片保留 command/description/workdir/output/exitCode/signal/running；区分运行、persistent provider、
后台 job receipt、结构化输出及有边界的正文末尾退出标记。非零退出与 signal 的卡片事实不覆盖原 stdout。
严格参数/权限字段验证和其它卡片字段规则按对应 derive 分支执行。
卡片 NSNumber 数字转换与 SEP 的 Double→精确 Int 规则不同；本轮用不启动窗口的系统 Foundation 数据探针，
核对 signed/unsigned、浮点截断/饱和和 bool 的原转换；meta 的整数读取仍排除 bool/小数。
这些样例不构成任意原始 JSON 数字词法逐字保真的保证，原结果全文仍保留。

其它变体保留原文件/搜索结果顺序、truncated/total、来源 URL/状态、读取行号与上下界、预期 diff 与实际 diff 的区分。
不把 opaque meta 的任意字段猜成 report/progress，不运行工具，不探查本机路径。

## 验证与仍未完成的部分

本轮 root workspace **234通过**；UI workspace **548通过、1项既有 #49忽略**；
root/UI all-targets 严格 Clippy、格式、依赖边界与实际公开 SDK 输入核验通过。
包含11项卡片用例、2项新 row 回归和1项新独立组件 consumer 用例。
本轮是受控公开事实、纯模型与同进程组件/宿主/第二 consumer 验证。具体统计、日志哈希和依赖输入见
[机器记录](rust-agent-ui-tool-card-20261022.json)。独立 consumer 验证 done 的原始 call 在不改其 phase 的前提下，
退出标记显示 error，输出仍留在详情而非动作标题。已有 owner/选区/焦点回归继续执行。

公开 TekesUI 仍为 `50ec49f3446924fdbc66b3cff22cd8cfe150fe7c`，八包243输入未变。
`regex = 1.13.1` 精确固定并同步两份 Cargo.lock，用于与源实现一致的 Unicode 退出标记边界；复用已有缓存。
AgentKit v0.3.8 为发布基线，本轮 API 是尚未发布的分支增量。

**结构化卡片视觉渲染尚未完成。** SDK 的 CodeBlockView/DiffViewRuntime 仍是被动绘制；
本轮未找到可独立消费、支持选择/原生文本映射的对应 composition Content。
原详情继续显示公开原始内容；不包装被动绘制来伪装完成，也不另建选区/字形引擎。
#25 延续已有富内容/语义 spans 契约，补充 code/diff 的基础消费场景；#47 图标/状态附件和 #48 ProcessContainer 仍开放。
完整 report/变更计数/连续工具组、目标 footer 严格可见性、完整 ClientKit 同内容布局和当前原生验收仍未完成。
没有新的真实 Native/Pi/DSH 故障、原生窗口、IME、VoiceOver 或跨进程证据。用户离开桌面，原生验证继续暂缓。
PR #10保持 draft，不合并或打 tag。未新建 target、复制验收程序或删除缓存，释放空间0。
