# 公开 reading/reveal 复核（2026-10-22）

重新查询 TekesUI 的全部公开 branches/tags、PR40 和 #22/#28：main 与最新检查点 tag
`remediation-toolbar-checkpoint-20261021` 均指向 `09b4bab44b6dfa65487250ea816b610b9df59095`，
与当前 AgentKit 精确依赖一致，无需再次修改 Cargo.toml/lock。PR40 已合并，其 CI 以及当前 main CI 成功。
#22/#28 仍 OPEN，但所需公开 reading/reveal API 已可消费，不能继续视为“未发布”阻塞。

正式接入沿用[持续阅读事务](rust-agent-ui-continuous-reading-20261021.md)与
[公开导航契约](rust-agent-ui-published-navigation-20261021.md)：SDK 拥有滚动、布局、clamping、
reading token 和焦点保护；AgentKit 只提供稳定业务 ID、版本及显式导航意图。没有复制引擎、
手算 offset、借 focus 实现 reveal 或修改 TekesUI。

本轮基于原分支与现有 Composer notice WIP 构建独立保留的验收程序；没有将 WIP 混入本次文档提交。
程序 SHA、原分支基点、全部 WIP 源码 hash、日志 hash 与收据保存在[机器记录](rust-agent-ui-public-reading-recheck-20261022.json)。
因此以下为该固定工作树程序的验证结果，不能替代提交后 CI 或稳定版验收。

- Cargo 实际解析的 8 个 SDK 包、241 个输入文件与公开固定清单完全一致；Rust UI 边界检查通过。
- UI workspace 455 项测试、全 target 严格 Clippy 通过，包含独立公开 API consumer。
- 原生 Sidebar 自动 reveal 返回 Completed：恢复、失败保留和选中行补读正常；同一 Composer、焦点及中文 marked baseline 保留，无提交。
- 原生 Run 分页 20→44 项：前插锚点、向上阅读、A/B 草稿与阅读位置、重连恢复通过；普通回合 Start、Latest 与 Jump 复用原控件，导航组字/选区保持。
- 系统简体拼音专项：仅向隔离 demo PID 投递按键，系统输入法将 nihao 组字并提交为“你好”；发送/导航屏障与单次发送通过，输入源前后不变。未用 Unicode 插入、粘贴或 synthetic marked-text 替代该专项。
- 现有 notice WIP 的原生刷新专项也通过，作为本地回归独立记录，不代表真实 endpoint 刷新成功。

分页/reveal 探针使用合成鼠标与原生文本服务 preedit；系统拼音是软件按键事件驱动的真实 IME。
均不声称物理键盘、候选窗或 VoiceOver 已验收，也不将 owned local SEP 夹具记为三 harness 的新验收。
本轮未发现新增基础组件缺口，不重复提交 #22/#28；#42 的跨新建 host 持久阅读恢复等已有缺口继续保留。
UI 仍为实验状态，PR10 不据此合并或打稳定 tag。

复用 `/tmp/agentkit-layout-text-292-target`，没有新建 target。保留约 66 MiB 验收程序及约 1.6 MiB 日志/metadata，
均为验收证据；本阶段未删除共享缓存，实际释放空间为 0。
