# 原生启动配置与真实端验证

本次不改变 Rust UI、presentation 或 SDK；新增的是原生验收驱动的显式启动配置与只读诊断。继续消费公开精确 TekesUI `09b4bab44b6dfa65487250ea816b610b9df59095`，2026-10-22 复核 main 未变化，[ProcessContainer #48](https://github.com/TekesApps/TekesUI/issues/48)仍 Open、无新增交付。

## 启动与证据契约

`scripts/test-real-session-endpoints.py` 默认仍直接启动 `--ui-demo`，保留应用真实退出码检查。显式传入 `--ui-native-app PATH.app` 时，原生阶段采用 `/usr/bin/open -W -n`；先校验 APPL bundle 的 CFBundleExecutable 确实指向同一个 `--ui-demo`。offscreen 阶段保持直接启动，不成为原生验收替代品。

LaunchServices 的 wait 返回值只表示启动器状态，不能当作应用退出码。每次输出 `NATIVE_PROBE_LAUNCH_RECEIPT`，分别记录 profile、binary SHA256、launcherExitCode 与 applicationExitCode；正常 app 启动的 applicationExitCode 明确为 null。必须另有且仅有一条原探针收据，原有每项业务/原生断言继续全部验证；即使 open 返回0，panic、缺失或重复收据也拒绝。没有自动降级、重试、强制激活或放宽焦点断言。

正常 app 启动仅传递 selftest 控制、截图路径及本驱动创建的本地 fixture 短期凭据，绝不批量转发当前用户的环境或服务密钥。收据不包含命令或凭据。stdout、stderr 使用不同文件以免独立写入偏移覆盖，随后合成为 `.log`；合并顺序不是跨流事件时序证据。原流、启动器输出均保留。wait 超时仍为失败，不能推断应用已经停止，须检查原进程后再决定下一步。

可另加 `--ui-native-observe-frontmost`。旁路仅查询 `lsappinfo front/info`，在 `.frontmost.jsonl` 记录系统前台 PID、bundle ID、ASN 与时间；不读取窗口内容，不请求激活，不生成输入事件。观察失败明确记为 unavailable，不伪造应用状态。它用于区分外部前台变化与原窗口状态问题，不替代原探针的实际 focus/marked/editor 校验。

## 本次真实结果

既有验收程序 `.runtime-fixtures/AgentKitCrossPaneCopy.app` 的 SHA256 保持 `2705f5f75f46f945ccf5ee4debc38351439f2b2d59d8ae66d95e45446fae5375`；UI 内容来自 `cbaae6f`，后续 `42c2835` 仅增文档。provider 使用隔离本地受控服务；Harness 是实际 Pi1.3.0、DSH0.2.1-rc.2、Native Kernel0.9.3，不使用计费模型。

| 验证 | Pi1.3.0 | DSH0.2.1-rc.2 | Native Kernel0.9.3 |
|---|---|---|---|
| 真实历史分页、A/B 阅读与草稿、重连 | 通过，50→56 | 通过，50→59 | 通过，50→56 |
| 活跃流式时向上阅读、原锚点/组字/编辑器、完成文字、一次提交与重连不重放 | 通过 | 通过 | 未完成，外部前台切换后4/9失败 |
| 真实流式回复、原输出行/文本 owner、选择/焦点、草稿隔离 | 通过 | 通过 | 未执行，前阶段失败 |
| 原 Send/Cancel、权威 aborted、受控 provider 错误、显式恢复 | 通过 | 通过 | 未执行，前阶段失败 |

以上都是同进程原生 ContentHost；不是 ProcessContainer 或 UI 跨进程验收。真实端基础 reading 模式不包含新增 rail 控件/组字的验收，该部分仍由[同程序的独立原生 fixture 与系统拼音验证](tekesui-reading-public-recheck-20261022.md)证明。流式阅读模式的中文 marked text 是合成文本服务输入；实体键盘、候选窗、VoiceOver 和整体配对视觉尚未验收。

## 失败归属与保留范围

- 首次正常 app 批次的 Pi 阅读在8/16失焦；未启用前台观察，不能认定原因。裸程序启动的历史失败也不因本次结果删除或改判。
- 带观察批次中，Pi、DSH四类原生验证全部通过；Native 流式阅读 PID35407 先正常 active，系统前台随后由该程序切换为浏览器 PID30172，探针4/9失败。它证实该次外部前台变化，不证明所有历史失焦的原因。
- Native 独立补跑 PID36464 未进入 active，整个观察期间前台为 `com.apple.UserNotificationCenter`，探针0/9失败。没有关闭通知、抢回焦点或放行后续测试，仍不计 Native 流式通过。
- Native 显式 `--kernel-ui-only`，不运行或声称 lifecycle/archive/child 协议验收。完整目标仍未完成，PR #10 保持实验草稿，不合并或打稳定 tag。

精确收据、源哈希、原生程序/Kernel 哈希、前台轨迹和各阶段日志在 `.runtime-fixtures/native-app-launch-20261022/`，摘要见[同名 JSON](rust-agent-ui-native-launch-20261022.json)。工具42项测试通过，包含 launcher成功但应用panic/无收据、重复收据、timeout保留日志、包身份和只读观察失败的拒绝路径。未改 Rust 源码，本轮不重跑既已通过的496项组件/宿主/第二 consumer 全套；提交后的 CI 独立记录。

复用原 UI 程序与根共享 target；headless cargo build 命中既有产物，没有新建 target 或 UI 编译。保留源码、WIP、固定依赖输入、程序、截图和全部失败日志；没有清理用途不明的缓存，释放空间0。
