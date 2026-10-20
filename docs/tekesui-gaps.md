# TekesUI 消费侧缺口

2026-10-20 当前状态：实验 UI workspace 已消费固定公开 8cf0840 的纯文本 Content API；#23 不再阻塞纯文本消息行。混合阅读仍待 [#22](https://github.com/TekesApps/TekesUI/issues/22)，可选中富内容与原生映射已提 [#25](https://github.com/TekesApps/TekesUI/issues/25)。现有 MarkdownView 是被动有界绘制，不在 AgentKit 复制其解析/选择/字形逻辑。以下早期七项与旧候选记录按时间保留；当前依据为 [正式契约](rust-agent-ui-contract.md)。

2026-10-19 正式pin更新：已固定公开4310937并完成独立消费复验；原始缺口/旧阻塞记录作为历史保留。最新逐Issue关闭结论与上游CI边界见 [正式pin复验](tekesui-public-pin-verification-20261019.md)。

最新候选消费状态见 [独立复验](tekesui-consumer-verification-20261018.md)：UI27/headless76、真实 Native SEP 到 GPU 通过，尚非原生窗口或正式发布验收。

2026-10-18 更新：TKUI-001/002 已消费最新全部 WIP 的 WindowWake / 版本化 ReadingController；下文保留原始需求基线，当前实现、通过证据与未验收边界以 [P0 联调记录](tekesui-p0-closeout-20261018.md) 为准。正式 pin 与原生最终体验尚未完成。

可直接交给 TekesUI 开发的优先级、接口需求和验收标准见 [七项开发任务](tekesui-development-tasks.md)。

只列需求与证据，不修改 TekesUI，不在 AgentKit 内补通用实现。基准是 ui/tekesui-source.json 的冻结开发快照。

| ID | 当前公开 API / 缺口 | 需要的事件或属性 | 对本轮影响 |
|---|---|---|---|
| TKUI-001 | Window 只有 UI 线程的 request_redraw；Rc/非 Send，没有后台唤醒代理 | Send + Clone 的 EventLoopProxy/WindowWake，后台请求合并为 UI Event；窗口关闭后安全失效 | 生产阻塞。异步网络变化无法在空闲窗口主动显示；demo 仅显式 --frame-pump 演示，不能作为修复或空闲性能验收 |
| TKUI-002 | ReadingController 支持尾追加与 UTF-16 重排锚点，但非尾追加视为新文档 | capture/restore 阅读锚点；支持 keyed item/message ID + 行内偏移，或带 prepend range 的内容更新 API | 生产阻塞。Earlier 的数据闭环可做，头部插入后的阅读位置连续性不可承诺；不另写滚动锚点算法 |
| TKUI-003 | TextFieldContent 使用 TextCommand，当前不处理 insertNewline:/insertTab:/insertBacktab:；ContentEvent::Key 没有完整修饰键/文本服务处理标识 | 多行编辑器的标准文本命令处理；明确的 IME-consumed/提交意图与 modifiers；composition-safe Tab 焦点导航 | demo 限单行、显式 Send。Enter 不发送，不在 AgentKit 内添加通用文本编辑行为 |
| TKUI-004 | 组合 Button 的 activated 是聚合结果，宿主通过公开 activations 计数取得具体动作 | 带 ComponentId 的 typed action/event callback，保留键盘/指针/AX 来源 | 当前可用稳定 ID 查询计数，不阻塞试点；扩展复杂组件前希望上游提供 |
| TKUI-005 | 可公开修改 TextInput 字段，但缺少受控值同步事务 | 外部值/版本更新 API，说明如何保留或重置 selection、undo、preedit，返回 applied/deferred | 当前按 destination 保留实例，组字期间拒绝外部替换；复杂外部草稿同步前需正式契约 |
| TKUI-006 | composition 指针事件未携带 modifiers/clickCount，TextFieldContent 内部固定 shift=false/clickCount=1 | 指针事件传递修饰键与点击次数，保留原生选择语义 | Shift-click、双击/三击选择未验收，不在消费侧实现选择算法 |
| TKUI-007 | TextFieldContent 使用固定表单视口，缺少可配置多行视口高度 | 多行文本视口尺寸/约束属性，与 TextSurface 阅读布局一致 | 不阻塞原型：Transcript 消费公开 TextSurface 完整视口；Composer 仅单行 |

已有能力不列为缺失：TextInput 的 preedit/UTF-16、选择、undo/redo；ContentHost focus；TextInputState 原生候选位置；ReadingController 尾追加保护；AX 节点/动作桥接。它们存在不等于真实物理输入法、VoiceOver 或性能已经验收。

不在本轮扩展：富 Markdown/工具卡片/附件、任务/子代理/SideChat/Summary UI；正式虚拟化、大历史增量投影、生产 launcher。先完成基础 Run 可验证部分，再以缺口清单协调 TekesUI。

TKUI-006 更新：公共完整指针入口、AgentText转交及原生demo适配已实现；共享双击字素/三击段落修复与消费者回归已通过。Gallery原生证据与AgentKit未验收边界见 [记录](tekesui-pointer-metadata-20261018.md)。表格保留初始缺口基线，不代表当前仍未实现。


2026-10-21后续观察：[TekesUI #27](https://github.com/TekesApps/TekesUI/issues/27)记录layout中inactive→active对缺失TextSurfaceHost的诊断时序；通用最小探针在只读2b16候选复现。它不替代原七项清单，也不是有效backend原生恢复的失败。生产宿主每帧必须提供文本服务，消费侧不补通用校验；证据见[Run叠层记录](rust-agent-ui-overlay-20261021.md)。
