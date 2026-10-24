# 真实 Native bounded pane 的原生阅读验收

本轮直接运行已有 Kernel **0.9.1 / v4** 验收程序，经认证公开 SEP 创建 owned parent，由真实 task 工具产生 one-shot child。模型服务为受控 loopback provider，执行配置禁止外网；未改 Kernel、未读写私有数据库、未手造 journal。

## 显式分页大小

新增兼容公开入口：`ChildRun::open_with_page_size`、`RunBinding::open_child_with_page_size`、`ChildPaneBinding::open_with_page_size`。已有入口保留签名及默认50。大小须为1..=200；非默认大小只允许内置 SameSession bounded source，自定义源自己拥有分页，独立子会话仍走原历史 lease。大小在此 source 生命周期内固定，用于初始读取、刷新、前缀再验证和 earlier 请求，不改变 scope、epoch、cut/revision 或执行能力。

这是未发布分支上的增量 API，不宣称已存在于 v0.3.8 tag。新增测试验证非法大小拒绝、独立 layout 不接收自定义大小、较小页的前插与刷新保留已加载消息，以及只读 binding 无 RunController；默认路径继续回归。

## 原生事实

新增 `--history-readonly-native-selftest --bounded-child-id <owned-id>`，要求显式 endpoint 配置、真实 Transcript 窗口，不接受 offscreen。此模式从配置的第一个 parent 打开其真实 child，并显式传1条分页；普通 demo 和本地fixture仍默认50。

runner 的 `--ui-native-readonly` 与 `--ui-bounded-child --ui-components --ui-transcript-content` 配套。正式程序接入公开 ChildPaneBinding、ReadOnlyTranscript 及原 SDK reading transaction。原分页/刷新控件激活后，检查原item和局部阅读位置、主会话中文草稿、不提交、无只读pane Composer/评价。

这次 one-shot child 只有一条长 assistant 消息。**可见行5→5，没有新增可见消息。** 原公开1条页的 `hasMoreBefore=true`；同一 frozen cut/revision 的更早页只有 `turn/start` 元数据，随后 `hasMoreBefore=false`。元数据前缀读取与刷新后锚点保持。真实收据明确记录 `realHarness=true`、`pageSize=1`、`historyExhausted=true`、`visibleRowsExtended=false`。

因此本结果补齐真实 Native 的只读pane原生阅读/刷新与元数据页边界证据；不能替代多消息child的真实可见行前插、真实流式阅读、实体键盘/IME、VoiceOver或配对ClientKit视觉验收。本地fixture已有38→75可见行前插证据；主Run已有三真实harness消息分页证据，三种范围保持分开。

早期原生断言错误地要求任何earlier页都增加可见行；保留失败程序和日志。新增公开原始页诊断还曾误带sessionId、误读event.kind，分别因请求路由和序列化字段错误失败，已改用无session注入的管理读取及event.type；不把诊断错误归因于SDK，不通过修改Kernel或读取私有库绕过。

## 复跑及证据

使用已构建程序和已有target，例如：

```sh
python3 scripts/test-real-session-endpoints.py \
  --kernel-root /absolute/TekesKernel \
  --kernel-target-dir /absolute/existing-kernel-target \
  --ui-demo /absolute/AgentKitNativeBoundedReading.app/Contents/MacOS/AgentKitNativeBoundedReading \
  --ui-components --ui-transcript-content --ui-bounded-child --ui-native-readonly \
  --ui-output /absolute/evidence/native.png
```

runner 仍验证真实子任务source-call、父子归属、重连、禁止作为独立可写会话打开，以及默认50的离屏组件路径。原生收据与离屏JSON分别输出，不能合并为同一验收层。

`.runtime-fixtures/agentkit-native-bounded-reading-evidence/` 保留程序/源码哈希、真实公开 initial/older 页、原生及失败日志、离屏截图、组件/API测试与Clippy记录。两个既有target继续复用，Kernel程序不重编译；debug0、incremental关闭，本轮无新target、删除缓存0字节。固定TekesUI仍为公开1294987d；不消费#43候选、不改Swift/ClientKit/Tekes工作树。

完整UI、持久checkpoint(#42)、多消息child前插和最终review/合并/tag仍未宣称完成。
