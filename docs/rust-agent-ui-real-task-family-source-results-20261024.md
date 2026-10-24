# #23：真实家族来源预检（2026-10-24）

这是接入准备与实际公开来源预检，尚未证明同一 demo 的 Task/child 家族导航、挂载或迟到分发。后续实施遵循[契约](rust-agent-ui-real-task-family-contract-20261024.md)，不以来源预检替代 #23 全部标准。

主会话审查了新 seed helper，纠正两项会造成虚假证明的假设：`childHistory` 返回 TranscriptSnapshot，不能当作含 sourceCall/parentSessionId 的原始有界页；工具调用本身也不能制造 canonical Task row。现在仅解析实际配对的 command/result，按 headless 公开 saveAs/JSON pointer 解引用，并通过实际 Summary.scope、Task/tool row、原父请求下 catalog 与真实 child 接收地址核对。Native sourceCall 通过额外公开只读 `subagents.history` 获取；不拆解 opaque UI/tool ID。manifest明确 `realHarnessAcceptance=false`、`uiAcceptance=false`，不授予控制权限。

使用本轮固定 headless 与共享三端 lifetime/provider，在隔离 SSD 状态运行实际请求，三端均成功匹配公开来源：

- Native0.9.3：实际 canonical Task.scope 保留父 Run，与 parent transcript task/tool 对应；原始 bounded page 回显实际 parent/child、source call、historyRevision，catalog mode=`bounded`。只读 child 不升级为独立可发送 Run。
- Pi1.3.0：实际 parent Summary.subagents 的 opaque来源ID/sessionId 与公开 catalog、原 parent/child-bound history 匹配；catalog mode=`continuable`、parentAvailable=true。未制造 Task 行，也未从ID存在推断发送权；后续仍由原绑定决定。
- DSH0.2.1-rc.2：实际 parent-bound catalog 的 continuable child 与 child presentation 的 canonical Summary.scope 匹配；无 Task producer 证据，task保持None。

新解析/拒绝测试12项；当前全Python **155 passed**。保留前一版本测试因断言错误文本过严而失败的日志，最终版本修正后通过。实际预检源码在执行前冻结，后核对未变；固定 runtime 入口、Kernel supervisor/worker 与原收据哈希再次匹配。证据位于 `.runtime-fixtures/real-multi-sidebar-20261024/task-family-preflight-evidence/`，包含实际命令、原始返回、manifest、执行脚本/源快照和身份文件，详见[机器收据](rust-agent-ui-real-task-family-source-results-20261024.json)。该Python增量没有运行Cargo或创建target。

这里建立的是实际公开请求/响应与来源链。新的 UI demo 必须通过自己的 RunBinding/epoch/frame 重新观察，不把 seed manifest 当 pane authority。下一步仍需在同一当前 demo 原 intent/background/dispatch 路径中完成 Task/child 打开、家族A→B→A、关闭、受控迟到投递和主Run/草稿保留。真实同Task row动态pointer producer未找到，不能用家族切换替代。桌面/chrome、物理IME、AX、ProcessContainer未运行，#23不关闭；#53门槛保留。
