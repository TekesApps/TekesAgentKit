# Pi 1.4.0 兼容升级（2026-10-23）

结论：Pi 1.4.0 通过 AgentKit headless 与 GPU 离屏回归，可作为主线程后续 Pi 验证版本。
1.3.0 保留为独立回归基线。未修改 TekesUI、ClientKit、Tekes 或 Kernel，未做桌面验收。
本轮改动仍在工作区，未发布 AgentKit tag，也未更改 Tekes 产品内置 Pi。

## 精确安装

- 候选：`/Users/frankqin/Library/Caches/TekesAgentKit/runtimes/pi-1.4.0`
- 基线：`/Users/frankqin/Library/Caches/TekesAgentKit/runtimes/pi-1.3.0`，原安装保留。
- 两者使用显式 `node/bin/node`，Node `v24.20.0`，不使用全局 Pi 或 latest。
- 候选使用 [官方 v1.4.0 release](https://github.com/earendil-works/pi/releases/tag/v1.4.0)
  中的 `pi-coding-agent-install-package.json` 和 `pi-coding-agent-install-package-lock.json`，
  分别保存为安装根的 `package.json`、`package-lock.json`，执行
  `npm ci --ignore-scripts --no-audit --no-fund`。直接依赖及所有 Pi 子包固定到 1.4.0。
- package.json SHA-256：`9792b34aba5a94320fde5c68600eac4a5720d0d63ebc527d462bd125665b29ee`
- package-lock.json SHA-256：`eb2c76ac270f099f2af3cbc6ccd08c3d22bf562b914af3690d1abba985461ade`
- 安装清单在候选根的 `agentkit-runtime.json`；完整证据索引见
  [JSON 收据](pi-1.4.0-compat-20261023.json)。该收据还记录 dist 文件树摘要，
  避免把两个版本相同的 160-byte CLI 启动器哈希误当作全部运行时代码身份。

宿主选择上面的候选路径即完成切换；不要覆盖/重命名 1.3.0 安装或修改用户真实会话。
回归数据与合成 provider 均使用本轮独立目录，未读取真实凭据或修改用户会话。

## SEP 变化与对照

`PiConfiguration::new` 的精确版本白名单从 `{1.3.0}` 扩为 `{1.3.0, 1.4.0}`。
保持实际 `--version` 握手与未知版本拒绝，不添加通配范围。
其余 Pi SEP 实现无需修改；新增专用 Python 回归 consumer 驱动。

只读对照了 ClientKit `PIHostEndpoint.swift` 的版本、原生 session 路径及身份契约，
和 Tekes Pi packaging 的宿主安装边界。Swift 的默认 1.3.0 契约未被本轮升级改写。

## 验证结果

| 范围 | 1.3.0 基线 | 1.4.0 候选 |
|---|---|---|
| 原有完整 headless 流程 | 通过 | 通过 |
| pointer / 历史 / adopt / 精确 fork / 缺失与替换保护 | 通过 | 通过 |
| 扩展问答、SDK 与模型工具子代理、Summary、重启与重连 | 通过 | 通过 |
| 历史分页离屏：28 回合、56 消息、重连与草稿隔离 | 通过 | 通过 |
| 扩展问答离屏：工具调用、回答落盘、单次发送、重连与 Summary 挂载 | 通过 | 通过 |
| 分支记录工具后新增扩展工具，再进程重启 | 复现工具未激活 | 通过 |
| 真实 SSE `length` 截断工具参数后继续调度 | 通过 | 通过 |
| 从截断调用与失败结果的中断边界恢复 | 复现孤立 `cut-call` 结果，被严格 provider 拒绝 | 通过，调用与结果均有持久化省略记录 |
| 真正模型压缩 Summary 请求及恢复后重启 | 被上述已知缺陷阻断 | 通过；两次请求均使用模型输出上限 24,576 |

专项中的中断边界从真实 Pi SSE 截断结果生成：保留官方公开 JSONL 到失败工具结果，
复制为独立 fixture，经 `session.adopt` 显式根授权恢复。没有修改原会话或访问私有数据库。
严格 provider 检查实际出站消息的调用/结果配对，不用 UI 可读或 RPC 成功冒充恢复成功。
1.3.0 的新专项失败是基线已知缺陷的复现，未被报告为全部通过。

额外 gates：workspace **310** 项测试、第二公开 API consumer **94** 项测试、
Pi all-targets strict Clippy、root fmt、headless 依赖边界和 diff whitespace 检查通过。
UI 程序仅构建 AgentKit 现有组件，TekesUI 固定依赖未改变。未做窗口交互、物理 IME、AX 或性能验收。

## 重放与证据

本轮固定程序与日志目录：
`/Users/frankqin/dev/TekesAgentKit/.runtime-fixtures/pi-1.4.0-compat-20261023/`。
程序 SHA-256、运行时身份和四份离屏收据摘要在旁边的 JSON 记录中。
离屏程序冻结后复用于两个版本；不把主线程后续 UI WIP 归为本轮修改。

```sh
python3 scripts/test-real-session-endpoints.py \
  --headless-program /absolute/tekes-agent-headless \
  --pi-root /absolute/pi-1.4.0

python3 scripts/test-real-session-endpoints.py \
  --headless-program /absolute/tekes-agent-headless \
  --pi-root /absolute/pi-1.4.0 \
  --ui-demo /absolute/tekes-agent-ui-demo \
  --ui-components --ui-transcript-content --ui-history-pages \
  --ui-output /absolute/fresh-output/history.png
# 另跑一轮：将 --ui-history-pages 换为 --ui-actionables。

python3 scripts/test-pi-version-compat.py \
  --pi-root /absolute/pi-1.4.0 --expected-version 1.4.0 \
  --baseline-pi-root /absolute/pi-1.3.0 \
  --headless-program /absolute/tekes-agent-headless \
  --output /absolute/fresh-upgrade-output
```

专项 `--output` 必须为新目录，保留 provider 请求、原生 JSONL、子进程输出和收据。
不加 `--baseline-pi-root` 可单独验证候选；1.3.0 加 `--recovery-only` 可越过新增扩展工具的
已知失败，单独复现截断恢复的孤立结果错误。

专项开发中修正过 provider 对普通 prompt 中 Summary 字样、压缩请求中旧扩展 probe 的误判，
以及“实时截断后继续调度”与“中断边界恢复”的断言混用。失败运行保留在 r1–r3 路径
（首次路径无 r1 后缀）；最终脚本按请求工具声明区分
agent 与压缩请求，并额外检查压缩专用 system prompt。候选最终成功记录为
`specialized-1.4.0-final/receipt.json`，升级连续性记录为 `upgrade-1.3.0-to-1.4.0/receipt.json`。

上述均为本地合成 provider 的真实 Pi 进程协议与离屏集成证据，不代表真实模型质量或桌面验收。
官方对应变更参见 [1.4.0 发布说明](https://pi.dev/changelog/releases/1.4.0)。
