# 实际 Composer 的留白焦点与 IME 门禁（实验候选）

本轮在 AgentKit 原 ComponentDemo 的事件门禁接入 TekesUI #34，仍复用原 Composer、RunView、presentation 和基础控件。没有增加编辑器、命中、焦点、IME、滚动或 Run 状态机。

## 固定输入和默认路径

独立解包核验 `a30b18db1a1df1f5a780c245080b22221a63cb20556f7274b76786d4d896584e`，497源/锁文件；归档 SHA256 `54b217691907389a0f9bc51bc8cafae4d54151dd2c491f32df7af5eaf25ded9e`。与未改95b140闭包相差5路径：原composition、交互契约及其聚合JSON、设计说明、SDK消费者测试。实际tokens crate和既有样式代码不变。

正式 UI pin 保持公开精确 `8cf0840790334ba224afc0595897c74231c712e6`；headless契约为AgentKit v0.3.8。默认pin尚无本查询，保留原门禁。`--composer-padding` 仅在隔离源码快照适配已核验候选，依赖显式的 `--turn-rail --forwarded-focus`；Cargo解析8个UI包均来自该解包源，不借用TekesUI活跃源码，不提升公开pin或关闭上游issue。

## 宿主责任

1. 当前帧成功render后，仅为当前呈现主Run与活动辅助Run内呈现的Composer调用原库 `set_padding_focus(ids.input_band, Some(ids.editor))`。整页折叠时不绑定不可用子树；恢复后再显式重绑。编辑器换源、卸载/重挂后重新声明。真实编辑器与动作仍属于同一真实输入胶囊，setup/context在外。
2. 组字门禁按同turn调用 `tree.padding_focus_target(event)`。只有返回**当前 composing 的原editor ID**时，才把原event转交原事件链。查询结果不保存为异步许可。`None`不放行整块band；原直接编辑器及显式非聚焦rail路径仍按既有策略处理。
3. 原 `event` / `event_with_text` 继续重检SDK当前路由。焦点、可用性、mount、overlay/modal/capture及真实动作命中由SDK拥有，宿主不复制私有判定或合成编辑器指针事件。

示例和变换都保留在 `scripts/test-reading-candidate.py` 的显式候选适配中。默认宿主只有清楚标记的接入位置，保持旧公开SDK可编译。接收侧观测位于真实门禁之后：只有原event实际进入原SDK时，才检查原focus/mount、edit baseline和marked selection，打印专项回执；吞掉点击不能生成该回执。

## 验收证据

组件专项覆盖实际宿主同编辑器留白转交、组字时发送/会话切换仍被阻挡、A/B/A草稿隔离、旧挂载绑定失效、成功render后重绑、折叠/恢复，并继续执行原Run/rail/Start及原胶囊按钮优先级/禁用/撤销声明检查。第二公开consumer调用原AgentKit接口；不是以SDK示例替代真实Kit组件。

独立原生fixture使用本接收侧固定输入与真实ComponentDemo。主Composer先组字 `中文组字`，实际点击胶囊留白后，由原NSTextInputClient确认selection1..3、marked0..4、原文本及子串/候选矩形查询。SideChat在原独立草稿后组字 `辅助组字`，真实点击自己的留白后确认selection14..16、marked13..17和原查询结果。两次转交均保留原editor/mount/baseline，主输入Send在组字时仍被挡住，结束组字后原发送、流式、取消、重连、任务/子代理/SideChat及草稿隔离回归通过。已查看辅助输入截图；原生进程以0退出，窗口关闭。

本项是工具驱动鼠标及合成AppKit marked text；物理输入法候选窗、真实三harness的UI分页/阅读锚点、#33焦点环像素/VoiceOver、完整ClientKit同名Views与配对产品视觉仍未验收。生产方的根WIP、GPU和native证据不记为本接收侧结果。旧候选及失败构建收据保留，完整检查、源码身份和本轮清理以同名JSON为准。

## 本轮结果与缓存

AgentKit候选全包374项（含专项22项、包装委托1项和第二公开consumer35项）、默认宿主18项、固定SDK controls/text438项及SDK公开consumer250项通过。默认与候选全目标strict Clippy、fmt及Rust UI边界检查通过；子集计数不与整包重复累计。此前提交1480c75的四项CI已全绿；本轮提交的CI另行跟踪。

在独占原Cargo target锁、检查待删文件无lsof占用、确认最终metadata不引用淘汰源码路径及独立原生exe仅动态链接系统库后，清理3个中间候选对应的133个deps缓存文件，释放约243MiB。保留最终原生exe、所有固定SDK/源码快照、失败编译日志、成功日志/收据/截图及最终依赖缓存；没有建立新target，继续复用原目录。
