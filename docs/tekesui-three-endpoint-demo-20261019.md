# 三端 demo 与最新共享契约（2026-10-19）

AgentKit UI demo 现在支持显式 Native / Pi / DSH 配置，统一使用已有 SessionRuntime、RunController、RunSurface。保持实验 UI，不增加通用文本/阅读算法，不改 TekesUI 或 Kernel。

## 当前源码基线

- AgentKit 公开 main / v0.3.7：`a8610a420bad72eaedcf706b30df87153a0f3e16`。
- WebUI v0.1.17：`8b9b11c`，其 client-contract 同样固定 AgentKit `a8610a4`。
- TekesUI 固定公开 commit `431093786de47b48cd566d0d9c0eafa088b10d68`，188 个消费输入哈希全部重查一致：`03721866a362b1d31261ebbbc86140eb07e9e23320871c325234a5212591213d`。
- 旧 e4 目录不变，当前构建不消费它。上游 #1–#14 后续 WIP 不隐式进入该 pin。

本地 main 从 `0dff9fc` 快进到公开 v0.3.7。先在仓库外备份全部 141 个源码/文档文件，再对 35 个上游变化文件三方合并；无冲突。128 个未涉及上游变化的文件在快进时逐个哈希确认不变。原 UI publication revision / fencing 改动保留；新配置和文档另行修改，仍未提交或发布。

## 与 WebUI 更新的关系

| 已合入的公共能力 | Rust UI 的责任/当前挂载范围 |
|---|---|
| v0.3.1 批注 DTO、编译、历史恢复 | 使用共享 compiler；不在控件解析/构造 prompt envelope。当前尚未挂批注卡片 |
| v0.3.2 线程动作定义/能力/执行 | 消费共享动作 ID、标签、可用性和执行结果；当前 demo 尚无线程菜单 |
| v0.3.3–0.3.4 置顶关系与 activity | 共享层持久化关系和状态事实，宿主/消费者负责分组排序。旧 pin order 不重新引入 |
| v0.3.5–0.3.6 provider live/reported usage | 数据进入共享 projection；不估算 token 或从实时增量重复累计 durable totals。当前纯文本 demo 尚未挂用量控件 |
| v0.3.7 完成回合 Fork | 仅消费权威 settlement 边界与共享策略，不推算最新可见 seq。当前尚未挂 Fork 按钮 |

WebUI 后续试验的“回退/编辑重跑”已撤销，不属于这次基线。不升级 Kernel 以重做 Fork。本轮闭环仍是连接、纯文本 transcript、基础 composer、发送/取消、流式、会话草稿和历史分页；不宣称所有 WebUI 功能已出现在 Rust UI。

## 配置与执行

`ui/demo/src/endpoint_config.rs` 只负责宿主配置和启动，组件层不依赖 harness adapter。配置示例见 [demo README](../ui/README.md)。

- `kind` 选择 native/pi/dsh；省略仍兼容原 Native 配置。未知 kind/字段失败，不回退 fixture。
- 两个已有且不同的 session 映射到 A/B，保留 endpoint/session 身份。demo 不创建真实 session、不选模型。
- Native 的 bearer、DSH 的 cookie/bearer 由指定环境变量取值。DSH 两种来源不能同时配置；空值、换行等非法 header 失败。
- Pi 显式绝对路径、独占 dataRoot、显式 environment / environmentFromEnv、extensions / sessionRoots，沿用现有 env_clear 和指针恢复规则。无全局会话或凭证扫描。
- 启动等待已有 readiness/history baseline；失败停止本次 runtime。不会自动重发 mutation。
- `--native-selftest` 仍只使用 fixture，不能组合真实 endpoint。离屏 stdout 仅含数量/状态，不含消息正文、凭证。

## 验证

| 检查 | 结果 |
|---|---|
| 主 workspace（含 Web bridge/共享契约） | 103 passed |
| 独立 UI workspace，含第二 public-API consumer | 36 passed（新增 6 个配置用例） |
| Python 交付/验收工具 | 16 passed |
| root + UI strict all-targets Clippy、fmt | 通过 |
| headless / UI 依赖与所有权边界 | 通过 |
| Native Kernel 0.9.1 / Pi 1.3.0 / DSH 0.2.1-rc.2 | 真实安装运行时、独立 SEP、合成本地 provider，三端 headless 验收通过 |
| 三端 explicit config → 已有历史 → RunController → GPU | 三个 UI_CONSUMER_LIVE_OK，均检查非 stale 且存在 final assistant，PNG 1000×760 |

集成复现：先构建当前 `tekes-agent-ui-demo`，再运行 `scripts/test-real-session-endpoints.py`，显式传入 `--kernel-root`（0.9.1）、`--kernel-target-dir`、`--pi-root`（1.3.0）、`--dsh-root`（0.2.1-rc.2）、`--ui-demo`、`--ui-output`。任意 runtime root 可单独使用；Native 输出保留指定路径，Pi/DSH 加后缀。脚本使用独立临时 HOME/dataRoot 和本地 provider，既有个人 session 不在验收路径内。

本机日志 `/tmp/agentkit-three-sep-ui.log`，三张图为 `/tmp/agentkit-three-sep-ui{,-pi,-dsh}.png`；状态、版本和图像摘要见 [机器收据](tekesui-three-endpoint-demo-20261019.json)。这些是开发调试产物，临时文件不作为发布依赖。

真实发送/回传/重连/文件与子历史由 headless 验收覆盖；这次三端 UI 接入检查配置、历史恢复和离屏呈现。共享 UI 发送/取消等交互由 fixture 自动化覆盖。未现场操作原生窗口，未重新验收物理中文组字、VoiceOver 或性能，未调用付费模型。TekesUI 既有 Gallery CI 失败由上游 #2 跟进，此处通过的消费者测试不代表整仓上游 CI 全绿。
