# 回合导航：宿主历史回调与588277原生 Start

此文保留历史588277候选证据。当前公开1294消费、正式宿主桥接与新原生验收见[公开版本回合导航](rust-agent-ui-published-navigation-20261021.md)；历史默认输入与阻塞不覆盖当前状态。

对照 ClientKit `RunView.jumpToTurnFromIndicator`、`ThreadTranscriptPresenter.semanticLastSectionID` 及 Tekes `ClientAgentHostAdapters` / `NavigationModel`，补齐主 Run 的显式历史输入和回调。AgentKit 捕获 Agent 回合的原来源；宿主仍拥有选择历史、Back/Forward、持久化和窗口导航，没有在 Kit 内重写 NavigationModel。

## 主宿主契约

每次当前帧成功 render 后，主 Run 宿主调用：

```rust
run.set_turn_navigation_context(Some(RunTurnNavigationContext {
    bottom_is_visible,
}))?;
```

`bottom_is_visible` 是实际显示事实，不能以运行状态或 sticky `is_following_latest` 推断。ClientKit 的近底显示策略与 sticky follow 是不同概念。SDK/宿主应供给实际底部可见事实；本轮诊断只使用已经确定在 exact latest 或普通回合起端、远离尾部的受控场景，不宣称已经完成产品宿主的通用近底分类。

Kit 选择原语义最后回合或第一条实际可见回合作为 from。原轨道 action 在事件发生时捕获 `RunTurnJump`，包含完整 RunScope、from/to；后来的阅读事实不重定向旧点击。`RunTurnNavigation::host_jump()` 仅供读取，不授予 endpoint 控制权限。

宿主先验证原 SDK reading attachment，再同步调用 `claim_turn_navigation_with_host`。它复用原 owner/publication/epoch/generation/pane/mount/一次性认领检查，然后按 ClientKit 次序调用：

1. `RunTurnNavigationHost::record_turn_jump(&RunTurnJump)`。
2. `take_pending_turn_reveal(&RunScope)`，丢弃本次显式点击已经满足的旧 held reveal。
3. 调用方按返回的 TurnStart / Latest 立即调用原 SDK，再请求 render。

record 回调必须只更新属于当前线程的宿主历史，不能在两个回调/SDK 调用之间替换 Run 或重新选择来源。失败不运行回调；clone 共享认领，重复不能再次记录。回调 panic 或之后 SDK 失败不是任意业务的回滚事务，不自动重试或降级。

Task、subagent、SideChat 不开启主线程历史，使用 `set_turn_navigation_context(None)` 与普通 `claim_turn_navigation`。不通过比较 endpoint/session 字符串猜测 pane 角色。关闭再开启主历史会退休旧手势；卸载/折叠清空上下文，来源发布后必须在新帧重新提供事实。没有捕获上下文的主历史手势明确拒绝，不自动退回无历史执行。`set_turn_navigation_context` 成功本身不是 native 帧发布证明，成功 render 仍是宿主前置门禁。

已恢复历史的程序 reveal 使用宿主已有待揭示目标和普通导航路径，不能再次调用 record 制造新历史停止点。本轮未实现宿主 Back/Forward 栈或声称其产品导航已验证。

## 原生普通回合

精确隔离 SDK 仍是 `5882774a0e73dfcb423efb4260c150e03f50f5cc9a87bd2331c1e171963faacd`，490 源/锁文件和默认8cf公开依赖不变。

runner 增加显式 `--turn-start-native`，需同时使用 `--turn-rail --item-navigation-native`。它只在隔离 adapter 中启用原 `Start(Axis::Vertical)`、零 margins 和正常 library motion；语义最后回合仍走原 follow_latest。没有反算 offset、focus reveal、另一个编辑器或 consumer 滚动引擎。

正式原生窗口中经 AppKit 合成 preedit，再实际按下/抬起原轨道的普通回合和 Latest 两个按钮。普通回合第一条真实内容行到达原可见 viewport 起端；两次动作各一次，host record/take 各两次，from/to/scope 与原点击一致；原 Composer 实例、焦点、输入 baseline 和 marked text 保留，无意外提交。最终专项还通过了组字内 selection1..3。截图分别记录 Start 与 Latest，未把生产方或先前 c779 的原生结论转记为本轮结果。

同一版本的默认路径349项、588277候选366项、候选 Run/rail/Start/host17项、第二公开consumer35项以及两条全目标strict Clippy通过；默认公开依赖的独立原生回归也通过。专项计数属于整包子集，不累加。这里的原生鼠标和文本服务都是工具驱动，不是物理输入法或实体键盘验收；不宣称实际三harness混合分页/阅读、原生undo/redo、VoiceOver、全部动画/RM矩阵或配对ClientKit产品视觉通过。

#33 原业务包装的焦点绘制层仍待固定输入和接收侧验证，不能把原按钮激活通过当作 ring 视觉通过。Composer留白焦点#30的新独立输入尚未消费。完整 Views、hover/ramp/preview、其他消息和搜索显示、真实三端UI、物理IME等完整交付目标继续有效；早期c779一次原因不明的末尾publication变化仍保留原记录，不以本轮通过反推其已解决。默认pin/私有冻结均未提升，PR保持实验状态。

## 构建缓存

本轮 default 与 candidate 统一使用已存在的 `/tmp/agentkit-layout-text-292-target`，debug0、incremental关闭，没有为每次源快照建立独立target。验证结束后，在无占用和Cargo target锁保护下删除旧 `ui/target/debug/{deps,build,.fingerprint}` 约20.5GiB；保留旧独立可执行文件，确认其仅依赖系统framework；所有本轮原生二进制、源码、固定输入、日志、收据、截图与未提交改动保留。详细终态、源哈希与清理收据见同名JSON。
