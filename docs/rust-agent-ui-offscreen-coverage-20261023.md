# 三端离屏覆盖补验（阶段开发增量）

基于 `baa2f55` 上的 Rust 开发增量，继续消费公开 TekesUI 精确 pin
`50ec49f3446924fdbc66b3cff22cd8cfe150fe7c`。2026-10-23 复核最新已发布
AgentKit 仍为 `v0.3.8`；本轮 UI 增量尚未发布。

当前程序对 Native Kernel 0.9.3、Pi 1.3.0、DSH 0.2.1-rc.2 分别完成：

- 28 回合历史播种、公开分页扩展、原 scope/frozen cut、另一会话草稿保留、
  重连后历史保留、无重复消息、旧收据失效。
- 公开 Composer/问答卡片、一次响应、canonical 答案记录、另一会话草稿保留、
  重连后新历史及不重复输入。

两种 profile、三个 endpoint 的六份 consumer 收据全部通过。工具/问答 profile
恢复的 canonical 工具数分别为 Native 1、Pi 1、DSH 2；历史 Activity 默认折叠，
这不证明工具行在原生窗口中可见，也不证明真实多 call 连续分组的窗口布局。
问答服务来自本地受控 provider，数据经过实际 harness 与公开 SEP，未连接生产模型。

## 修复的离屏验收断言

初次问答补验在 Pi 触发 `7 != 8`：旧诊断把 canonical 消息数直接等同于可见消息行数。
实际有一个空容器被隐藏。修复位于 demo 的 `transcript_coverage`，不改变业务状态机。

新的检查按 canonical owner 核对所有输入/输出和可见 body/fragment；隐藏输出必须
无正文、推理、附件、错误或系统提示，并且每个工具引用均能解析到原 ToolActivity。
未知块、未解析工具引用、应显示正文不能被当作空消息跳过。
原失败日志和程序保留，没有把失败记录改成通过。

修复后，问答 profile 的消息核对为：

| Endpoint | canonical 消息 | 可见消息 owner | 已证明隐藏的空 owner |
| --- | ---: | ---: | ---: |
| Native | 7 | 6 | 1 |
| Pi | 8 | 7 | 1 |
| DSH | 11 | 9 | 2 |

35 项 Transcript 测试通过，新增回归要求缺失的未知内容、未解析引用及正文必须失败。
严格 Clippy、格式和 UI 边界检查通过。前一轮全 workspace 614 项通过、1 项既有
TekesUI #49 忽略的结果保留；本次新 demo 诊断没有用前一轮全量结果冒充重新全量运行。

## 证据与限制

证据目录：`.runtime-fixtures/global-tool-runs-20261023/`。

- `offscreen-coverage-summary.json`：六份收据的字段核对与哈希。
- `offscreen-coverage-exits.json`：两个 profile 的退出码和程序 SHA-256。
- `coverage-source/`、`coverage-source-manifest.json`、`coverage-source.patch`：315 个源码输入及当前 WIP。
- `transcript-coverage-tests-final.log`、`coverage-clippy.log`：本次组件与静态验证。
- `actionable-offscreen-driver.log`：首次失败，保留；`*-coverage-driver.log` 为新程序补验。
- `*.consumer.json/stdout/stderr`、`*.png` 和两份程序：保留原始收据、离屏图片及程序。

复用同一共享 target，没有创建 `.app` 或新 target；没有实际签名、桌面截图、
权限申请或原生窗口验收。本轮释放空间 0 字节，源码快照约 8.5 MiB，保留供复现。
Native 使用显式 `--kernel-ui-only`：Kernel 生命周期和子历史协议未在本轮重验。
阅读锚点、物理中文 IME、跨进程及完整 Views/视觉对齐仍未验收；
TekesUI #48/#25/#53 的既有上游门槛保持，不能据此合并或打完整交付 tag。
固定宿主及截图入口遵循[工具链说明](rust-agent-ui-native-toolchain.md)，实际签名安装仍待进行。

## 累计 frame 更新补充回归

`display_order` 现有 23 项通过，严格 Clippy 通过。本轮新增真实 journal 增量场景：
同一 response frame 重发旧 b/c 调用并追加 d，随后逐条返回 a/b/c/d 结果、输出最终正文并结束回合。
检查明确证明消息 frame 的 source position 已改变，旧 call 的原位置仍保留；
旧 ToolActivity 地址、原组头 ID 和 ButtonContent 地址保留，组内只增加一个新 call。
每一步均保留 Composer 焦点和中文 preedit baseline，旧 prepared update 被拒绝，
第二公开 consumer 查询仍返回同一原控件身份。回合结束并重新展开历史后仍成立。

证据为 `cumulative-frame-{tests-final,clippy}.log`、`cumulative-frame-receipt.json` 和
`cumulative-frame-display_order.rs`。与六份离屏程序的源码快照相比，仅新增测试文件内容；
运行时实现没有变化，因此本轮未重复生成离屏程序或跑桌面验收，也未重建独立 target。
