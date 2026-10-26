# 公开 TekesUI 的原生回合导航

当前精确依赖仍为 `1294987dacabb33c0e411c13ee5ea544a1123f8e`，公开 tag `remediation-checkpoint-20261021`。2026-10-21 再查公开 tags、#22/#28 及其讨论：issues 仍 OPEN，但当前 pin 已包含所需 reading/reveal、Exact/Press、preserving_focus 和 Start API；没有等待活动工作树，也没有再次升级到未经审核的输入。Cargo 实际八包241输入校验继续通过。

对照当前 ClientKit `RunView.jumpToTurnFromIndicator` 与 Tekes `ClientAgentHostAdapters`：AgentKit 提供共享 `navigate_run_turn`；主宿主明确提供历史回调，辅助 pane 明确选择 Auxiliary。原 SDK attachment 校验发生在业务认领及回调之前，随后按 record → take pending → 原 SDK 导航执行。旧 publication、epoch、mount、pane 或重复 clone 不能重放回调。之后的 SDK 失败不回滚宿主任意副作用，也不自动重试。

普通回合使用 `Start(Axis::Vertical)`、零 margins；语义最后回合使用原 `follow_latest`。正常 SDK 边界钳制仍有效，不能假定尾部空间不足的回合总能顶对齐。没有 offset 补偿、焦点 reveal、编辑器替换或新的滚动状态机。

原生 demo 安装真实 TurnRail，使用原 CompoundButton 的 Exact/Press、Content 的 preserving_focus，并转发原焦点装饰 hook。轨道具有独立 reading attachment，初次 Nearest 到最后 tick、目标改变时按源 comfort zone Centered，均不改 Transcript 的阅读文档。相同 membership 的 refresh 保留原 rail token；失败需要显式重试或新的有效输入。

宿主仅在成功 render 后发布主 pane 的实际 near-bottom 显示事实（ClientKit 40pt proximity），Kit 从实际可见回合/语义最后回合捕获来源。不能用 sticky follow 或 running 推断。demo 只记录历史回调，不实现产品 Back/Forward。

## 验证范围

- UI 全套425项（含第二公开 consumer）、阅读专项22项、脚本26项及 strict all-target Clippy 通过；`run_reading` 22 项涵盖分页/append、原始 Start clip、高消息、独立轨道、事件时来源、once、mount/epoch/publication fencing 和预编辑保持。
- 新增公开 consumer 测试：只更换 SDK attachment 身份时旧 token 在任何历史回调之前拒绝；新 token 可执行一次，clone 不可重复。
- 原生/GPU `--components --transcript-content --reading-native-selftest`：实际分页按钮和共享请求，20→44 项；向上阅读锚点、原编辑器、焦点、marked baseline；A/B 独立草稿/阅读、重连恢复；随后普通 rail 鼠标点击 Start 与 Latest 原 AX 激活，中文组字和选区1..3保留，两次宿主历史回调，无提交。
- native 第一次选择近尾回合，顶对齐断言受合法边界钳制影响失败；保留程序/日志，改用尾部空间充足的普通回合验证。未改 SDK 或增加像素修正。

以上 native 文本服务和鼠标由工具合成；不是本轮物理输入法、候选窗、VoiceOver 或完整产品视觉验收。Native/Pi/DSH 的真实分页/保留阅读沿用上一份[真实三端阅读证据](rust-agent-ui-retained-reading-20261021.md)，本次不把 local fixture 的新导航测试记为三端新验收。

新建树后的持久 checkpoint 仍跟踪 [TekesUI #42](https://github.com/TekesApps/TekesUI/issues/42)。完整 rail hover/preview、富内容及全部 Views 对齐尚未完成；PR #10 保持实验草稿，不据此合并或打稳定 tag。

证据保留于 `.runtime-fixtures/agentkit-navigation-evidence/`：程序哈希、原生日志（含失败记录）、Cargo metadata、测试/Clippy日志与源码哈希。验收程序为 `.runtime-fixtures/AgentKitNavigationCheck.app`；均不进入发布依赖。复用 `/tmp/agentkit-layout-text-292-target`，debug0、incremental关闭，没有新建 target。本阶段未删除用途不明确的共享缓存；删除字节为0。
