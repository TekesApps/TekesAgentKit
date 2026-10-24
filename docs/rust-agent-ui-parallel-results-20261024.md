# 主会话与三个子会话的本批结果

基线源码 `8b5d45b`；PR #10；并行分工及互斥写入范围见[执行安排](rust-agent-ui-parallel-work-20261024.md)。主会话完成代码审查、集中串行构建、独立 consumer、协议补验和本收据；子会话不独立推送、不创建 target、不运行桌面。

| Issue / 子会话 | 交付 | 仍待完成 |
| --- | --- | --- |
| #22 Sidebar | `6c700e9`：固定多 endpoint 聚合 binding；按完整 address 保留同名 ID、独立 selected-read、一次性动作票据、原 endpoint 路由与替换实例门禁。两项聚合单测、三项双端夹具测试通过。主审补正 async capture 和夹具独立 endpoint 身份，未放宽 runtime 身份校验。 | demo 宿主接线、跨端查询/过滤分页整体组合、终态 observer 的同实例恢复、原生菜单/AX。旧单端 API 保持兼容。 |
| #23 SidePanel | `8b5d45b`：定向状态/pointer/父导航审计；未发现确定产品缺陷。demo 的四套相关测试共 41 项通过。 | 明确记录的状态/导航组合、完整当前窗口/chrome 和原生验收；不要求无依据的笛卡尔积测试。 |
| #25 Summary | `9a62950`：工具结果 UUID 仅为候选，须同端已知 session 精确或唯一后缀匹配才授予导航；无 store、父自身、带前缀父 UUID、unknown 与歧义保持被动。独立主审发现的父 UUID 重定向已修复并回归。 | SDK 视觉项、最终布局/可达性；真实 Native 在本批未重跑。显式宿主 facts 的真实性仍由宿主负责。 |

主会话集中验证：presentation **225 passed**；UI workspace **634 passed / 1 项原有忽略（#49）**；UI workspace/presentation 全 targets 严格 Clippy、根/UI 格式、边界及历史矩阵本地证据核验通过。

两个真正独立 Cargo consumer 均编译运行成功：一个构造并挂载 RunPort/Sidebar/SidePanel，另一个编译新 binding 公开 API 并运行空集合/未知 endpoint 配置拒绝。后者不是有效双端运行证明；双端行为由原夹具测试证明。每个 consumer 的六个 SDK 包均为精确 `50ec49f`，实际 checkout 的 243 个已审输入逐哈希匹配；各自 source/manifest/seed/final lock/metadata 保留。没有导入 demo wrapper。

为确认 Summary 门禁不会禁用合法 pointer，当前冻结程序另运行 Pi **1.4.0** 与 DSH **0.2.1-rc.2** 的独立子 Run profile：原 Summary pointer → SidePanel/RunPort → 原 Send/Cancel、流式子回复、父历史隔离、草稿恢复、重连不重复及旧权限撤销均通过。driver exit 0。使用真实 harness 协议和隔离的合成本地 provider，不是生产模型证明。`physicalIme/nativeInteraction/crossProcess/readingAnchorAcceptance` 仍 false，`completeRunPort/completeTranscript/richContent` 没有提升。两张 PNG 是程序自身离屏渲染，不是桌面截图；本批没有 Native Kernel 实际协议重跑。

四个程序与 390 个保守源码/Cargo/SDK manifest 输入已存档并在运行后逐哈希复核不变。该清单是保守输入集合，不是编译器 trace；外部 consumer 自有输入另记。[机器收据](rust-agent-ui-parallel-results-20261024.json)保留程序、依赖、原始回执及哈希，完整证据在 `.runtime-fixtures/parallel-issues-20261024/`。

上一个 PR head `52288a0` 的两套 CI（[40046605891](https://github.com/TekesApps/TekesAgentKit/actions/runs/40046605891)、[40046600843](https://github.com/TekesApps/TekesAgentKit/actions/runs/40046600843)）已复核并归档原始产物：headless、UI workspace tests/fmt/Clippy、独立 consumer 均成功；严格 #53 阅读仍 accepted=false、父追加 offset=634、详情不可见。它们不是本批新提交的 CI；本批推送后的 CI 另外检查，不合并、不放宽门槛。

所有 Issue 保持开放。没有新增构建 target，没有删除缓存，释放空间 **0**。共享 target 是本批验证/后续 CI 对照仍需的现用缓存；源码/WIP/固定快照、历史证据和当前程序/锁/日志/回执/离屏 PNG 均保留。桌面、系统 IME、签名安装、权限申请及 ProcessContainer 仍不执行。
