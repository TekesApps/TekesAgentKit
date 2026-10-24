# #29：真实三端分类与历史保留的离屏闭环

关联 [#29](https://github.com/TekesApps/TekesAgentKit/issues/29)、#22、#14、[PR #10](https://github.com/TekesApps/TekesAgentKit/pull/10)。[实施契约](rust-agent-ui-real-sidebar-classification-contract-20261024.md)与[机器收据](rust-agent-ui-real-sidebar-classification-results-20261024.json)分开保存。只改 AgentKit Rust，ClientKit、Tekes、Kernel 和 TekesUI 均未修改；固定公开 TekesUI pin 仍为 `50ec49f3446924fdbc66b3cff22cd8cfe150fe7c`。

## 实现

只读复核当前 ClientKit `c289bc9d33edcd83a782ac17927a8c65a027eccd` 的 `ClientSQLiteSessionStore.isMainThread`，仍为 origin!=subagent 且非 ephemeral；Tekes `8858ccd9d46590852ec7a0949aee289fc6798d70` 的宿主契约测试保留 fork、排除 child/scratch、workspace threadCount=2。未运行 Swift 构建，也未修改上述仓库。

`SidebarClassificationFacts` 对原 summary 的 origin、parentSessionId、ephemeral 区分 Missing / Known / Unknown，未知 JSON 原样保留。仅 fork/subagent 是已知 origin；字段缺失不提升为 main，显式 false 不等于 missing，parent-only fork 不排除。

在原 inventory item 的同一轮读取中，捕获原成员谓词结果和字段；经原 SidebarPublication、multi publication、搜索/宿主 packet 透传。source revision、connection epoch、observer generation 保留原发布身份；失败/重启的 retained publication 不重标为新事实。`archiveFacts` 来自同一 MemoryProjectionStore 快照；缺失字段不输出该键，显式 null/false 原样输出。收据拒绝错误 endpoint、revision/generation 和不匹配的 records/活动库存/成员关系。

全局 canonical history 没有被 Sidebar 筛选删除。原 selected read 仍使用同一成员条件，不能补回 child、ephemeral 或已归档地址。分类元数据只描述来源，不授予导航、写入或 pane 权限。

## 实际验证

当前程序连接 Native 0.9.3、Pi 1.3.0、DSH 0.2.1-rc.2，全部使用隔离 home/data 和本地合成 provider。通过公开调用生成普通来源、永久 fork、归档 fork，以及支持的 ephemeral；Pi/DSH 使用实际模型工具生成的两个 child。没有写私有 harness 历史。

| 原始来源 | 生命周期样本 | 同帧 active records | 实际归档来源 |
| --- | ---: | ---: | --- |
| Native | 4 | 5 | archive 回执；无 workspace inventory，保留 unknown |
| Pi | 6 | 8 | archive 回执及 ready workspace archived IDs |
| DSH | 5 | 5 | archive 回执及 ready workspace archived IDs；保留 summary 合法 |

15 个公开生命周期样本逐一比较非空 canonical transcript、scope、history revision/throughSequence 与稳定内容。归档退休 journal 时允许 freshness 变为 false，缓存内容必须保留；随后公开 unarchive→原库存恢复→reconnect→open 后，原历史再次恢复。DSH 不生成没有公开能力的 ephemeral。

Pi 的 scratch 属于 endpoint 实例，新实例会清理旧 scratch。因此分别验证上一实例 scratch 已退休，以及在 demo 当前原 Pi runtime 内通过公开 fork 新建 scratch：原 RunBinding 持有非空历史，原库存明确 ephemeral=true，原 Sidebar 不显示且 selected read 不能补回，过滤前后原 transcript 相等。没有把旧 scratch ID 改写成新 ID，也没有用旧实例历史冒充跨实例持久性。

同一个当前离屏 demo 收到 22 个原 selected reads；原六组家族目录/导航/子历史/返回关闭仍通过。另独立重跑九会话 Sidebar：limit 5→10、挂载 6→9、行 ID 稳定、原焦点保留、查询和 reveal 目标挂载均通过。离屏挂载不是桌面上的实际可见性。

本地检查：

- UI workspace 747 通过、0 失败、1 项既有 #49 忽略；最后的离屏 probe 生命周期补充随后另做普通 demo build、严格全目标 Clippy 和实际三端运行。
- Root workspace 340/0/0；root/UI 严格 Clippy、格式和依赖边界通过。
- Python 170 通过，包括 9 项合成拒绝测试；合成输入不充当真实来源证据。
- 独立公开组件 consumer 实际执行通过，六个挂载；精确 pin 和输入闭包沿用原审计。

## 原始证据与限制

SSD `.runtime-fixtures/real-multi-sidebar-20261024/classification-attempt9/` 保存逐端公开命令/返回、原始 delivery、acceptance、版本和程序 SHA。`classification-source9-identity.json` 的 519 个来源文件在运行后逐一复核不变；源码快照和归档 headless/demo 程序保留，历史程序只归档。

之前失败的 attempt1–7 完整保留：Native Task 来源默认 fork 曾返回 internal，原因和修复均未证明；归档态直接重开语义流失败，不将缓存保留称为归档态重开。Native workspace 等待、DSH summary/回执假设、通用单端 proof guard 和收据字段命名等验证工具错误均保留独立失败证据。修正验证器不是修复上游 harness。

仍未运行桌面、物理 IME、OS AX、ProcessContainer；同 Task row 的真实动态 pointer 仍属于 #23 后续范围。TekesUI #53 仍开放，严格阅读门槛不变；不合并、打 tag 或发布。新提交 CI 另行核对，不沿用 b368cd4 的通过步骤。

所有构建复用现有 SSD shared-target，没有新增 target 或安装 harness。本批没有删除缓存（实际释放 0 字节）；保留当前验证依赖、固定源码和验收证据。此前磁盘迁移的 67.97 GiB 是当时本机目录移除的分配量，不作为本批清理量。
