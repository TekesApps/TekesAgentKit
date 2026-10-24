# Sidebar 同实例 observer 恢复契约

关联 #22。原 SharedAgent.follow(state) 是公开的持续内存读取入口，可显式重新订阅，不需要 Kernel、TekesUI 或协议变更。恢复仅指重开观察订阅，不调用 runtime reconnect、不重试任何 mutation。

每个 SidebarBinding 保留原 watch sender、固定实例身份与单调 observer generation；初始 Observing，终态 Failed，只有捕获的私有原端 retry token 可进入下一代 Restarting。重复、旧代、跨实例 token 拒绝。返回 Ok 只表示接纳重试，恢复成功须等新代合法帧。generation 与 publication revision 均 checked increment；锁内不跨 await，旧代即使迟到也不能发布。

Restarting 保留 loaded、旧 rows、activity anchor 和错误诊断并标 stale；新代合法完整帧才替换。已因首次失败收敛的 aggregate 不倒退为未加载；不伪造 endpoint loaded。持续 running 的活动位置通过原 InventoryRows 保留。Drop 只终止 observer，不终止运行中的 Agent。

原 aggregate owner 保留。故障端重试后其旧 action ticket 与 selected-read receipt 永久失效，健康端继续有效。动作在 admission 前复核原端 generation、当前 publication 和 live definition；已经接纳/发出的 mutation 不撤销、不重发。selection_generation 与 observer generation 各守原语义。

子会话分别处理多端 token/action/status 组合与 selected-read 的来源证明；主会话处理单端生命周期、终态注入回归、公开 consumer、宿主接线与验证。测试注入只放在绑定内部 cfg(test)，不增加生产强制失败 API。桌面、系统中文输入、真实多 harness 和完整 Sidebar 关闭标准仍单列。

库存转换中的宿主动作回调在生命周期锁外执行；提交前重新检查 generation/failed/publisher。Drop 先关闭本地发布资格并取消最新任务，不能仅靠 abort 阻止正在执行的旧回调发帧。计数耗尽关闭 publisher，调用者须把 watch closed 当终态；不循环复用 revision。

`3d594a8` 阶段提供共享公开 API 和来源门禁，当时 demo 的用户触发恢复入口仍须另行接线。公开 API 验证不代表已经有原生重试按钮或完成真实断流验收。

## 实现与验证

实现 `3d594a8`，PR #10。主会话重构单端 reader；两个新子会话分别提供多端适配和 selected-read 来源证明；独立只读审查后，主会话再收紧宿主回调重入与 Drop 门禁。

最终 UI workspace **664 passed / 0 failed / 1 既有忽略（TekesUI #49）**，严格 all-target Clippy、格式和静态边界通过。绑定库 12 项中包含 6 项 observer 生命周期回归，以及聚合 Restarting/convergence 回归。覆盖原 watch subscriber 恢复、旧正常/失败帧拒绝、非完整 baseline 不清旧行、首次失败不伪造 loaded、running anchor、并发双击/跨实例/旧失败代 token、计数耗尽、Drop、宿主回调在锁外读取 observer 状态。双端相同 session ID 测试验证 A 恢复只退休 A 的旧 action/selected receipt，B 的原票据仍实际返回原端 Copy outcome；新 A 票据重新可用。

协议 fixture 提供真实 SessionRuntime baseline；终态失败与迟到帧仅通过绑定内部 cfg(test) 私有函数注入。恢复后使用原生产 SharedAgent.follow 读取 fixture runtime，计数确认没有增加 connect、provider follow 或 mutation 调用。这证明本地 reader 生命周期，不是模拟真实 provider 断网成功。

独立外部 package 实际编译公开 observation/retry API、运行无效配置门禁；有效恢复 IO 由上述绑定库测试另证，不扩报为 consumer 的运行范围。普通 demo build 成功，未启动窗口、未接用户重试控件。只增加测试用 futures-util 依赖边，复用锁中已有版本；SDK 精确 pin 保持 `50ec49f3446924fdbc66b3cff22cd8cfe150fe7c`，实际 6 个包、243 份审查输入重新 hash 一致。

证据根 `.runtime-fixtures/sidebar-host-20261024/`：`sidebar-observer-final-workspace-tests.log`、`sidebar-observer-reentry-clippy.log`、`public-observer-consumer-run.log`、`sidebar-observer-demo-build.log`、`sidebar-observer-source-manifest.json`。程序 `agentkit-sidebar-observer-demo` 与公开 consumer 独立归档；[机器收据](rust-agent-ui-sidebar-observer-recovery-20261024.json)给出来源和日志 hash。重入修正前的完整 suite 单列为 before-reentry 日志，不当作最终源码验收。

全部构建复用共享 target。确认没有编译进程、候选文件无占用后，只用现有 Cargo 锁的单一非阻塞 exclusive flock，清理被新测试替代的旧 binding 单测程序和 dep-info 两个文件；target/deps 的 du 实测减少 **1,474,560 bytes**。源码/WIP 核对一致，当前测试程序、验收程序、固定快照与日志保留；其它归属不明的共享产物未删，未整删 target。清理明细见机器收据及本地 observer-cleanup-receipt.json。

demo 用户重试入口、真实多 harness 故障组合、Mirror FTS/pins、远端 cursor、分页/reveal 像素、桌面/中文物理输入/AX/ProcessContainer 继续开放；本批不关闭 #22、不合并或打稳定 tag。


## 后续用户入口

后续实现按[observer Retry UI 契约](rust-agent-ui-sidebar-observer-retry-ui.md)接通原始 packet、状态子组件及 demo 路由；上述 664 项历史收据仍只适用于 `3d594a8`。新实现的源码与验证单独记录，不倒填旧程序或旧收据为新 UI 验收。
