# #25：DSH Summary 标题与完成来源

关联 [#25](https://github.com/TekesApps/TekesAgentKit/issues/25)、#27、[PR #10](https://github.com/TekesApps/TekesAgentKit/pull/10)。[短契约](rust-agent-ui-dsh-summary-source-contract-20261024.md) · [机器收据](rust-agent-ui-dsh-summary-source-results-20261024.json)。只改 AgentKit，仍消费公开精确 TekesUI `50ec49f`；UI 保持试验状态。

## 修复与真实来源

旧程序在真实 DSH0.2.1-rc.2 中把带 description 的 JSON 字符串 arguments 显示成“Subagent”。baseline 是已归档的858a63c程序；其 Summary 源码与7e9383b完全相同，不冒称该程序构建于7e。ClientKit c289bc9 的 SummarySubagentPresentation 会解析一次 JSON 对象，再取 brief/description，缺省用 producer headline；Tekes8858ccd宿主只提供导航等适配。

本次 Summary 仅对 task/subagent/subagents 局部读取对象或单层 JSON 对象字符串。Subagent 使用 brief→description 并 trim，缺省复用已有 presentation_title；Task 保留 task_name 的原文本规则。坏参数仍保留原子行，不递归解码、不改写 Transcript/history，不扩大 pointer/status 权限。

最终真实 DSH 运行17条公开命令、7次隔离合成 loopback provider 请求。标题正确显示“Catalog proof child”，原 arguments 仍是字符串。原调用位于父 Journal seq16/contentIndex0，逐项配对原 assistant/message 的 callId/name/arguments 与 seq19 的 result；不拆 opaque rowId。重连前后 epoch2→5、原 child pointer 保留，公开 childHistory 同时有 initial/continuation，且不含父 probe。ownedDSH退出0后才写 accepted。

原 child inventory **没有 status 字段**；同次 authority 的数值最新边界是 seq26 turn/end reason.kind=completed，且非运行/blank、无队列/parked/actionable。显式 DSH 验收 profile 记录这条来源，原策略显示 completed、无状态标记；没有填造 raw status，也未放宽 Pi 默认验收 profile。此项证明同帧内存构建输入，不证明服务端多流事务一致或抵抗整个 producer 伪造。

## 验证与证据

- 默认 root346/0、opt-in presentation254/0、UI747/0/1既有忽略、Python204/0。新增5项 Rust 与7项 DSH 来源/拒绝测试覆盖实际参数形态、标题回退、原参数不变、pointer门禁、最新数值边界和缺省字段约束。
- root feature/UI 严格 Clippy、格式、边界与验收矩阵通过；普通 demo 构建通过。
- 原 witness.summary→SummaryInput→SummaryView→ContentHost 实际挂载，标题、原action scope和completed→无标记语义一致。只证明组件挂载，未渲染glyph/像素。
- 新独立公开 consumer 构建/执行 exit0，6个公开控件挂载与原断言通过；其程序本身不输出stdout，空stdout不是一份自报成功收据。运行日志、退出状态、程序SHA和137文件闭包 `721c1d36…` 已保留并核对。实际编译的 TekesUI243文件闭包仍为 `3ad8aa98…`。

SSD `.runtime-fixtures/dsh-summary-source-20261024/` 保留旧baseline、首次测试编写错误与修正日志、verified及加强来源配对后的final、原stdout/stderr/witness/commands/退出记录、1047文件运行输入快照、最终程序与hash。程序只归档；固定依赖安装继续保留，不创建新应用身份。后续文档与提交另作hash绑定。

真实 running/failure 转换、此profile父草稿隔离、最终glyph/hover/主题布局、桌面/物理IME/OS AX/ProcessContainer仍未验收，#25/#27不整体关闭。上一提交7e9383b两条CI已核实仅#53严格阅读门失败；本提交CI需独立检查，不能借旧CI称通过。不合并/tag/发布。

全部构建复用SSD共享target，无新增target，本切片删除缓存0字节；历史196.4MiB清理不重复计入。没有读私有harness历史、付费模型或修改上游，保留其他任务进程与WIP。
