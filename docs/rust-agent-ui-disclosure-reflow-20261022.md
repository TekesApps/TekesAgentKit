# 工具/推理与重排阅读验收

当前精确公开 TekesUI 仍为 `09b4bab44b6dfa65487250ea816b610b9df59095`，本轮未升级依赖、修改SDK或新增业务状态机。
对照 ClientKit `TranscriptDisclosureInteractionTests` 的原工具/推理展开及更新保留行为、
`TranscriptNativeSurfaceView` 的原presentation/live回传路径，以及 Tekes 宿主的窗口/toolbar所有权。
此项补充原正式 Transcript/RunView 的阅读验收，不宣称完整工具卡片分组、富内容或逐项视觉已对齐。

## 已验证

新增 opt-in owned fixture `with_reading_details()`，沿公开SEP history产生assistant reasoning、tool/call及tool/result，
由原runtime/projection/RunController规范化。不读取真实文件，不执行工具，不向真实provider发请求，默认fixture不变。

CoreText组件回归通过第二public-API consumer的 `drain_local_run_actions`，只排出一次原SDK动作并调用原RunView策略。
先展开原工具和推理，随后阅读后方稳定输入行：

- 已展开、处于视口上方的原工具结果增长80行，成对Run发布继续走原SDK reading事务。
- 宽度600/900/760、字号倍率1/1.4/0.85/1重排，原稳定行及相对可见内容区位置保持，误差≤1逻辑点。
- 原编辑器focus、中文marked baseline、选区和展开工具的原文本控件保持；不恢复follow_latest，不产生Send/Cancel等会话操作。
- 结束组字会正常产生该原目的地的草稿Edit。收起工具/推理后正文子树消失；显式AX激活离屏控件会由SDK聚焦/reveal，属于主动导航，不冒充被动重排的锚点断言。

原生入口：`--components --transcript-content --disclosure-reading-native-selftest`。
仅允许owned local fixture，不与真实endpoint/offscreen/其它专项组合。实际GPU/原生窗口中：

- 使用原SDK reveal声明目标，再通过原按钮的合成鼠标点击展开推理和工具。
- 显式选择后方阅读行并在原Composer开始中文preedit（selection 2..5）。
- 通过公开selftest AppKit BeginLiveResize/Resize/EndLiveResize改变窗口至1000×740、1400×860；完成一次resize后再进入下一次。
- 改变原宿主TextBackend字号倍率1→1.4→1，继续使用SDK文本/布局。
- 每个阶段核对原阅读行/相对位置、停止跟随意图、原编辑器地址、focus、marked baseline和无提交；展开详情仍在原树中。

原生收据已通过。此专项使用合成鼠标、原生文本服务preedit与resize事件，不是物理拖窗、真实输入法候选窗或VoiceOver。
native验证的是owned协议历史及展开后的重排；视口上方工具结果增长是组件层证据，不能扩张为三真实harness工具流验收。

## 边界与失败记录

初次组件用例错误地要求Unmark后没有任何intent；实际为原草稿正常Edit，现严格核对其目的地/正文并拒绝其它操作。
随后错误地要求显式AX激活离屏控件仍保留之前的阅读行；SDK原focus路径会揭示控件，现将主动导航与被动重排分开验证。
两次失败日志保留，没有修改SDK、添加offset补偿或更换原控件以消除测试失败。

全UI workspace 456项测试、全target严格Clippy、fmt及Rust UI边界检查通过，原生最终程序和源码hash见同名JSON。
SDK闭包沿用当前8包/241输入清单，未使用活动兄弟工作树。没有新基础组件缺口，不重复提issue。
完整工具/推理样式、真实三端工具流与重排组合、paired ClientKit视觉验收和全部UI目标仍开放。
Composer完整setup依赖的toolbar事件/逐项语义缺口仍跟踪TekesUI #36/#5，不以另一套宿主按钮/菜单状态机替代。

复用 `/tmp/agentkit-layout-text-292-target`，debug0/incremental关闭，没有新建target。
源码、固定依赖、验收程序及失败/成功日志保留；本阶段未删除不明确的共享缓存，实际释放空间0。
