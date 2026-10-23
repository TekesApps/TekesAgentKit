# 三端真实混合工具离屏闭环

在 `ea36946` 的原 canonical 控件上新增独立 `--ui-mixed-tools` 验收入口。
使用现有 RunBinding、原 Send、RunPort 和 TranscriptContent，不新增状态机、SDK 控件或桌面宿主。
本轮只修改 demo/验收夹具及文档；Kernel、TekesUI、生产模型及系统权限均未修改。

Native（Kernel 0.9.3，显式 UI-only）、Pi 1.4.0、DSH 0.2.1-rc.2 均通过：

- 本地受控 provider 先回推理与两次 read，再在真实结果回传后发第三次 read，最后返回正文。
  文件是隔离临时目录的只读测试文本；读取由各 harness 执行，未模拟工具结果。
- 三个 canonical 原 ToolActivity 在 live 更新与完成期间身份/地址保留；展开后组成一个连续工具组。
- 原 Composer 在流式更新和重连期间保留焦点及合成 preedit，组字期间 Send 被阻止；另一会话草稿不变。
- 重连后的权威历史只含一份本轮输入和三个工具调用，推理与真实文件内容返回均被断言。
- 最终原 Transcript 离屏 GPU 输出与源覆盖检查通过，用户靠右、机器靠左。

这是同进程离屏和合成 preedit 证据。`nativeInteraction`、`physicalIme`、
`readingAnchorAcceptance`、`completionProven` 均为 false，不转移旧原生程序的通过结论。
Native 本轮排除完整生命周期和子历史协议验收。

## 夹具修正与失败证据

首轮探针曾用更新中的 binding 状态检查旧 UI 帧，改为检查 RunPort 实际发布的同一帧。
DSH read 的参数名为公开 schema 中的 `file_path`；Native 严格 schema 要求显式 offset/limit。
Responses 完成帧必须包含已完成的调用；后续请求可能只带本次工具结果，不再带原输入或全部旧结果。
macOS 临时文件路径使用 resolve 后的 `/private/var/...`，与 Kernel 已规范化的许可根一致。
上述修改均在测试 provider/探针中，不放宽业务断言，也未改变 harness 权限。

所有原始失败日志、超时快照、成功收据、PNG、程序及源码摘要保存在
`.runtime-fixtures/mixed-tool-runs-20261023/`。最终 Native 为 attempt-9，Pi/DSH 为 final；
早期失败不得解释为通过。确切身份和收据见[JSON](rust-agent-ui-mixed-tools-20261023.json)。

验证：Python 74 项、demo lib 4 项、严格 demo all-target Clippy、fmt、静态 UI 边界通过。
生产组件本轮未改，此前全 UI 616 通过/1 忽略及第二 consumer 证据仍按其原阶段解释。
构建仅复用 `/tmp/agentkit-layout-text-292-target`，未创建新 target；历史验收程序与失败证据保留。
固定原生宿主工具已交付，实际安装/签名、桌面验收和 TCC 状态核验仍未执行。
