# 三端原生补验（2026-10-22，部分通过）

源码为 `4b7511f0151f87d48413b3e4cfa9a77729ddb4ff`；继续消费公开 TekesUI
`50ec49f3446924fdbc66b3cff22cd8cfe150fe7c`。复用正常 APPL 验收程序，SHA256
`2696969bb4d0a27804acf9fbe48d4cb06b974ff67f571b030b9098936fae497e`。
程序与四项记录源码哈希复核一致；没有重编 UI、另建 target、改断言或强制抢回焦点。

## 结果

| 原生窗口检查 | Pi 1.3.0 | DSH 0.2.1-rc.2 | Native 0.9.3 |
| --- | --- | --- | --- |
| 公开分页、前插/刷新锚点、A/B阅读和草稿、重连恢复 | 通过，50→56 | 未完成，phase 4 失焦 | 通过，50→56 |
| 反馈原控件、中文备注、合成组字发送/导航屏障、会话草稿保持 | 通过 | 通过 | 通过 |
| 原发送、流式预览、原输出选择及焦点保持、结算、重连不重放 | 通过 | 通过 | 通过 |
| 原取消、权威中止、受控 provider 错误展示、显式恢复、单次提交 | 通过 | 通过 | 通过 |
| 流式追加时向上阅读、原 editor/marked baseline、重连阅读 | 未完成，phase 2 失焦 | 未运行 | 未运行 |

各项来自独立完整收据；不是单次三端全绿。Native 使用明确的 `--kernel-ui-only`，
不包含其 lifecycle 或真实有界 child 验收。提供商是本地受控测试服务，协议运行时是真实 harness。
分页实测是公开 page 请求，不能代替 fixture 中更完整的原分页按钮、回合 reveal 和导航组字检查。

## 失败与边界

最初反馈窗口出现额外 `ce` 的失败日志仍保留。本轮 Pi、DSH、Native 原断言通过，
未再次观察到该文本；这不能证明其原因已修复。

人确认桌面空闲后仍记录了前台切换：Pi stream-reading 两次都在 phase 2 切回 Codex；
DSH reading 在 phase 4 切回 Finder，随后 Codex。原 probe 在限时关闭时断言未达终态。
没有把 LaunchServices 的返回码 0 当作应用通过，也没有自动重激活、重试提交或放宽焦点条件。
前台变更原因尚未确定，不能归因于用户或 TekesUI。后续应先定位前台变更来源，再补完整组合验收。

本轮为同进程原生事件/文本服务及合成 preedit；不证明实体键盘、输入法候选窗、VoiceOver
或 ProcessContainer 跨进程能力。此前独立真实系统拼音收据仍按原范围保留，不转移到本轮检查。
TekesUI #48 等上游缺口仍保持；没有修改 TekesUI 或复制其基础实现。

## 证据与资源

[机器收据](rust-agent-ui-native-resume-20261022.json) 包含每项原收据、各阶段退出码和日志/截图 SHA256。
原始证据在 `.runtime-fixtures/` 下的 `three-native-exclusive-20261022`、
`dsh-native-exclusive-20261022`、`kernel-native-exclusive-20261022`、
`pi-dsh-native-actions-20261022`、`three-native-stream-reading-20261022`。
失败不覆盖，后续通过使用独立目录。未删除验收证据或可执行程序；本轮释放空间为 0。
复用现有共享 Cargo target，driver 的 headless 构建为现有产物检查，不新增独立缓存。

PR #10 仍为 draft；补验前的 `4b7511f` 四项 CI 均通过。此文档新增原生证据，不扩大 CI 的
离屏检查范围，不构成合并或 tag 的完整验收结论。
