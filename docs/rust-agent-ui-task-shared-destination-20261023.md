# Task 展示共享目标状态

对照 ClientKit `c289bc9` 的 `ComposerSurfaceContext.swift`：草稿/附件/注解/设置的
身份是 ComposerDestination；主区、分栏、完整 SidePanel 不参与身份。
Tekes `RunUserControlsGraphTests.swift` 使用同一 ComposerDestinationStateStore，并保留
TaskRunPanelProjectionState 的独立异步解析门禁。两仓仅作只读参照，未修改或编译 Swift。

Rust 原 `TaskPaneBinding::for_run` 只复用 runtime，再创建新 RunController；同一完整 scope
会分出另一份草稿。本轮红测试得到空草稿，期望父 Run 的非空中文草稿。

现在 `for_run` 在权威任务行恰好指向原 RunScope 时复用现有 controller/presentation。
内部 Arc 只延长现有观察器寿命，不建立第二个 presentation、输入引擎或宿主草稿缓存。
每个 Task 仍有独立 history lease、加载代次与控制权限；source 关闭或 pointer A→B→A
不会复活旧回调，也不会关闭原 Run。异步准备失败会按原票据流程恢复共享草稿。

新增公开 `open_resolved_from(&original_run)`：宿主已有对应目标时显式提供原 binding，
同时校验 runtime 身份和完整 scope。存量挂载不能暗中换到另一份 controller；
异步 history 返回后的并发安装也检查该身份。第二公开 consumer 实际调用此入口。
独立任务仍需权威行；不能从工具调用猜造 pointer，不能把只读有界子历史提升为可写 Run。

完整 UI workspace 622 通过、1 项既有忽略，Python 74 通过，严格 Clippy、格式及边界通过。
Task 定向 9 项、附件 7 项均通过；初次完整检查中两项旧附件隔离断言失败的日志保留，
按同目标共享和旧上下文拒绝的契约修正后完整重跑通过。

定向验证包含同目标草稿/发送准备共享、父 mount 卸载后的订阅重连、原控件编辑、
精确 scope/runtime 拒绝、关闭后的准备票据恢复、A→B→A、祖先 Task/SideChat 控制权失效、
有界只读拒绝与旧 history baseline 栅栏。日志与源码摘要收据见
`.runtime-fixtures/task-shared-destination-20261023/` 与同名 JSON。

本轮是 fixture/组件/第二 consumer 验证，没有新真实 harness 会话、桌面验收、截图、
签名安装或权限申请；此前真实三端收据保留原程序身份，不能转移给本轮源码。
TekesUI pin 仍为 `50ec49f`；#48 ProcessContainer 与 #53 阅读门禁仍 OPEN。
`37ad08f` 的 PR/push CI 均为 headless 成功、严格 nested-detail-reading 失败；不豁免、不合并。
