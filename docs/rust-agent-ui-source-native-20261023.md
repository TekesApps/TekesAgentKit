# 当前来源修复的原生续验（2026-10-23）

源码 `2243a8e` 使用现有共享 target 构建；本轮冻结程序 SHA-256 为 `4ac85b5860cb693a1f49988b17d4bc53c1ba60d9b40a3cecc40a9dfc8899ccdf`。通过 LaunchServices 打开独立 APPL bundle，前台身份轨迹与原始 stdout/stderr 保留。未将旧程序收据移用于本次源码。

- 本地 fixture 原生阅读：历史 20→44、公开分页请求、前插锚点、向上阅读、焦点与 editor、组字状态、A/B 草稿隔离、会话阅读恢复及重连恢复均通过；无提交。
- 系统简体拼音：原输入源不变，定向向已核验前台进程投递按键，`ni hao`→`你好`；marked text、组字发送门禁、会话导航门禁、提交后单次发送均通过。程序与来源 SHA 与阅读测试一致。

首次阅读调用误填预期收据名称，wrapper 报错；原应用的完整成功收据及日志保留。修正为原程序 `NATIVE_AGENT_READING_SELFTEST_RECEIPT` 后重跑，严格 launcher/crash/唯一收据及字段断言通过，没有降低门禁。

这是本地 fixture 和软件按键的原生窗口验证，不代表真实三 harness 原生交互、物理键盘、候选窗、VoiceOver 或 ProcessContainer 验收；也不证明尚未接入整帧的 thinking TranscriptContent/RunPort。完整范围仍以[交付状态](rust-agent-ui-delivery-status.md)为准；TekesUI #53/#48 门禁不变，PR #10 保持 draft。

复用 `/tmp/agentkit-layout-text-292-target`，不创建新 target；验收程序用硬链接保留，未分配第二份可执行文件内容。未删除缓存，释放空间 0 字节。源码/WIP、依赖、原验收程序和证据均保留。精确路径与证据摘要见[机器记录](rust-agent-ui-source-native-20261023.json)。
