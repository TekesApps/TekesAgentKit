# #17 / #23 / #27：实际独立子 pane 的来源边界

基线 bd98bd0；之前 primary-child-run-only 的成功不代表 SidePanel。只读复核当前 RunBinding/ChildPaneBinding：独立 child 通过原 RunController::for_child 保留 parent.thread_id + child.session_id、直接父 session 操作归属与祖先 pane 权限。不得在验收中另建普通 controller、重选 scope，或把后读 runtime authority 拼接到旧快照。

第一步只在 ui/bindings 增加默认关闭的 acceptance-source-witness feature，透传 presentation 同名 feature。RunBinding 的显式验收方法捕获前后检查 access 非 Missing，调用其已有 controller.refresh_with_source_witness，返回原 RunSourceWitness；没有 controller 的有界/只读子历史明确拒绝。接口不暴露 controller、不重新获取 lease、不切选择、不复制投影逻辑。生命周期检查与 SessionRuntime 原帧各有锁域，不宣称两者构成服务端事务证据。投影降级保持原 projection_succeeded=false，不升级为成功。

先验证真实 ChildPaneBinding::for_run 的嵌套父 scope（thread、直接父 session、child 各异）、原 watch 与返回 Arc 同一实例、authority 不随后续更新改变、pane与祖先退休拒绝、只读拒绝、父选择和草稿不变。复用现有 runtime mock和控制接口，不能测试另一套状态机。

后续第二步再从原 demo 无窗口 proof 调用真实 pane binding，并增加显式 feature 的独立公开 consumer。第一步接口/组件成功不等于真实 Pi/DSH pane 恢复已验收。默认 consumer 继续验证 feature 关闭边界，构建串行使用既有 SSD target，固定 TekesUI pin 不变，不运行桌面/IME/AX，不改外部仓库。
