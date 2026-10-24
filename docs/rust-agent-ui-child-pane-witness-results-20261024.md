# #17 / #23 / #27：实际子 pane 来源接口第一步

按[契约](rust-agent-ui-child-pane-witness-contract-20261024.md)，ui/bindings 增加默认关闭的 `acceptance-source-witness`，只透传 presentation 同名 feature。RunBinding 的显式方法使用其既有 controller 捕获原 RunSourceWitness；前后检查 pane access，Missing 或无 controller 的只读/有界 child 拒绝。不重选 scope、不新建 controller、不重取 lease，不拼接快照或复制投影。生命周期与 runtime 来源分别有锁域，不宣称服务端事务一致。

使用真实 ChildPaneBinding::for_run 的合成公开 SEP fixture 验证嵌套 thread/直接父session/child 三者不同，原 run/selection/summary scope不变；返回 snapshot 与原 watch 为同一 Arc，父选择/草稿保留，authority 不随后续 inventory 更新而改变。pane及祖先退休拒绝，SameSession只读拒绝；畸形原Journal导致投影失败时仍保持 projection_succeeded=false/stale/禁止提交。主会话补齐了 fixture 流持有、scope 和降级回归，没有改生产状态机。

定向2项、feature bindings **35/0**，默认完整 UI **750/0/1**；默认 workspace 和显式 feature 严格 Clippy/格式检查通过，普通 demo build通过且未启动。独立 workspace 的公开 consumer 在默认及显式feature两种配置均执行成功；后者从公开接口检查方法类型，运行时的原 pane捕获由前述组件测试验证。default cargo feature tree 确认无 witness feature，不能把 workspace feature合并当默认能力。

CI增加专门的 opt-in bindings Clippy/test 步骤；第二consumer的实际fixture源码、锁文件和日志封存。SDK公开243文件闭包不变，root实现及Python未改，复用bd98bd0相关证据。本轮新提交CI另验。使用共享SSDtarget，4份定向测试/consumer程序仅归档，输入1085文件manifest与源码增量保留于 `.runtime-fixtures/child-pane-witness-20261024/`，见[机器收据](rust-agent-ui-child-pane-witness-results-20261024.json)。没有新target，缓存删除0 bytes；不删除当前仍依赖的默认/feature编译缓存。

这只是接口与合成原pane来源验证。实际Pi/DSH的demo feature透传、真实子pane来源捕获、失败重连/分页和pane草稿生命周期仍待第二步；不追认此前 primary-child-run-only 的成功。桌面/物理IME/OS AX/ProcessContainer、最终布局与完整ClientKit对齐未完成，#17/#23/#27保持open，#53严格门仍保留，不合并/tag/发布。
