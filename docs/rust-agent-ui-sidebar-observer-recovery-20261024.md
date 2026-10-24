# Sidebar 同实例 observer 恢复契约

关联 #22。原 SharedAgent.follow(state) 是公开的持续内存读取入口，可显式重新订阅，不需要 Kernel、TekesUI 或协议变更。恢复仅指重开观察订阅，不调用 runtime reconnect、不重试任何 mutation。

每个 SidebarBinding 保留原 watch sender、固定实例身份与单调 observer generation；初始 Observing，终态 Failed，只有捕获的私有原端 retry token 可进入下一代 Restarting。重复、旧代、跨实例 token 拒绝。返回 Ok 只表示接纳重试，恢复成功须等新代合法帧。generation 与 publication revision 均 checked increment；锁内不跨 await，旧代即使迟到也不能发布。

Restarting 保留 loaded、旧 rows、activity anchor 和错误诊断并标 stale；新代合法完整帧才替换。已因首次失败收敛的 aggregate 不倒退为未加载；不伪造 endpoint loaded。持续 running 的活动位置通过原 InventoryRows 保留。Drop 只终止 observer，不终止运行中的 Agent。

原 aggregate owner 保留。故障端重试后其旧 action ticket 与 selected-read receipt 永久失效，健康端继续有效。动作在 admission 前复核原端 generation、当前 publication 和 live definition；已经接纳/发出的 mutation 不撤销、不重发。selection_generation 与 observer generation 各守原语义。

子会话分别处理多端 token/action/status 组合与 selected-read 的来源证明；主会话处理单端生命周期、终态注入回归、公开 consumer、宿主接线与验证。测试注入只放在绑定内部 cfg(test)，不增加生产强制失败 API。桌面、系统中文输入、真实多 harness 和完整 Sidebar 关闭标准仍单列。

库存转换中的宿主动作回调在生命周期锁外执行；提交前重新检查 generation/failed/publisher。Drop 先关闭本地发布资格并取消最新任务，不能仅靠 abort 阻止正在执行的旧回调发帧。计数耗尽关闭 publisher，调用者须把 watch closed 当终态；不循环复用 revision。

当前阶段提供共享公开 API 和来源门禁，demo 的用户触发恢复入口仍须另行接线。公开 API 验证不代表已经有原生重试按钮或完成真实断流验收。
