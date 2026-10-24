# 当前组件版本的真实 Native 原生补验（2026-10-22，实验状态）

此前 [LaunchServices 原生批次](rust-agent-ui-native-launch-20261022.md)中，Native 流式阅读
因前台切换未完成，其后的流式发送与 Send/Cancel/error/recovery 未执行。本轮使用当前
AgentKit `03e2050` 的生产组件，以及固定公开 TekesUI
`09b4bab44b6dfa65487250ea816b610b9df59095` 补验。所有会话及 provider 均为隔离本地受控测试，
没有调用付费模型、修改 Kernel 或读取 harness 私有库。Native 明确使用 `--kernel-ui-only`，
不执行或声称完整 lifecycle/archive/child 协议验收。

## 当前结果与程序归属

初始程序为上一轮冻结的 `AgentKitPanelLauncherGrid20261022.app`，SHA256
`06404b6150d6b25eece03e47217ebb2db2c0b67ffdca5f13881826ab40140fb3`：

- 真实 Native 历史50→56条，public page、前插锚点、向上阅读、原编辑器、A/B阅读和草稿、
  同版刷新、重连恢复通过。该模式不验证 rail/marked baseline，不沿用 fixture 的 true 标记。
- 活跃流式开始后向上阅读，观察3次追加；原 item/local anchor、原编辑器及合成中文组字/焦点、
  另一会话草稿、完成全文、一次提交和重连无重放通过。
- 真实发送、活跃 preview、原输出行与文本 owner、AX文字选区、settlement 焦点、
  会话草稿隔离及重连不重放通过。此模式不验收阅读锚点。
- 组合的第4模式在输入 ERROR 文本时 `ExpectText` 得到 `text=None`；原生轨迹显示该程序
  激活后，前台切到 `com.tekesui.apphost.settings.dev`。整个初始批次退出1，不能称完整通过。
  前3项已有各自完整成功收据，保留该范围，不因后续失败丢弃或扩展。

## 验收探针修正与独立完成

仅修改 `native_run_controls_selftest.rs`：沿用现有 stream probe 的 active 门禁，不在窗口
inactive 时进入下一动作阶段；点击原 Composer 后，等待原 SDK focused()==editor，再派发
ERROR/RECOVER 文本。原45秒 deadline、真实 original click/输入/发送/取消回调、错误文字查询、
全部业务断言及最后 phase9 收据保持。没有强制激活、重建编辑器、直接写入运行结果或放宽断言。
这只是在发起动作前检查真实状态，不宣称可以阻止所有事件排队后的外部焦点切换。

新冻结程序 `AgentKitNativeControls20261022.app`，SHA256
`0626acdffa7d322be4ac4883c6e45735704fa48e29623a8d6c39bd79e7574727`；生产 components 源码
没有变化，组件与探针源码哈希记录在本地 `controls-program.json` 并逐项核验。
独立新会话模式完整通过，退出0：

1. 原 Send 进入活跃 preview，原 Cancel 得到真实 `aborted`，重连保持且不重放。
2. ERROR 单次提交，真实 provider 失败在 Indicators 与 Transcript 中可见，AX查询匹配公开源
   文案 `The provider ended the response with an error.`，无乐观伪造错误状态。
3. RECOVER 显式发送后取得 `ENDPOINT_REPLY_OK`，清除先前失败，重连不重放。
4. 同一会话恰好三个原输入；另一会话仍为空历史并保留自己的中文草稿。

完整 strict all-target Clippy、demo build、fmt及冻结程序/源码哈希检查通过。没有生产组件或
公开 API 修改；上一轮515组件/第二 consumer测试仍保留原程序范围，本轮不伪称重跑全部套件。
提交后的 CI 独立验证。没有发现可归因 SDK 的新缺口，不为外部前台切换建立泛化 issue。

## 范围与保留

Kernel supervisor/worker 为已安装0.9.3，source revision
`09508a9083c24c7accefc99516739f279b85913e`，supervisor SHA256
`365eca9385f36d5897a81ba398e7f34b4b9819896d6a1b78c618a7710b14380f`，worker SHA256
`1284014bd0a8d3cb711c0512b10762322a19af4beabc1f9e681bacb1e28fa64d`。
真实 Pi1.3.0/DSH0.2.1-rc.2 的对应四类原生证据仍见前次文档，并保持其原程序归属；
本轮没有重跑它们或宣称当前新程序的三端原生全套通过。

本轮均为同进程原 ContentHost；ProcessContainer #48、实体键盘/候选窗/VoiceOver、
长期稳定性、完整视觉和全部 ClientKit Views 对齐仍未完成。一次软件系统拼音通过仍见前轮
[Launcher记录](rust-agent-ui-panel-launcher-20261022.md)，不能由本轮合成组字替代。
PR #10 保持 draft，不合并、不打 tag。

初始失败、各模式成功收据、前台轨迹和日志在 `.runtime-fixtures/native-current-20261022/`，
两份 app 均保留。[机器记录](rust-agent-ui-native-current-20261022.json)分别绑定证据与程序。
复用既有UI与root targets；未新建 target、未删除缓存，释放空间0。保留源码/WIP、固定输入、
验收二进制、成功和失败证据。
