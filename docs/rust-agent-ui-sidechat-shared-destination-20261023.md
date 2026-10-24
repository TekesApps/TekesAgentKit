# SideChat 同目标重开

本轮对照 ClientKit `c289bc9` 的 ComposerSurfaceContext、ClientSideChatRunView 与
ClientSideChatCoordinator：同目标的展示位置不参与草稿身份；读不到 fork 行不能变成空白新会话；
确认丢弃/确认不存在才清理目标草稿。Tekes 的 RunUserControlsGraphTests 使用共享
ComposerDestinationStateStore。两仓只读，未修改或编译 Swift。

原 Rust `SideChatBinding::open` 每次创建新 RunController。demo 保留已有 pane binding，
掩盖了其他宿主重复 open 时获得空草稿的问题。新增第二公开 consumer 通过原 pointer/open
路径复现，红测试得到空文本而非非空中文草稿，未依赖 demo 的 pane cache。

现在 SideChatBinding 仅保留原 typed RunBinding，以完整 parent/scratch scope 为键。
重复和并发打开汇合到同一个 controller；不复制草稿/附件/设置、不新建 pointer 或输入状态机。
所有权、phase 与关闭仍来自原 SideChatController。每个原 binding 继续持有其 history lease。
Closing/Unknown 保留原可读状态和限制；读失败不当作不存在；确认丢弃/不存在后释放 owner
保留的 binding。对外仍有旧句柄时按原 access() 返回 Missing，不恢复旧控制权。
直接低层 RunBinding 构造仍是独立实例，宿主共享 pane 应使用 typed SideChatBinding::open。

组件回归覆盖：并发 open 汇合、同目标非空中文草稿、共享发送准备、所有展示句柄卸载后重开、
重连、Closing/Unknown 的同 controller、确认关闭后的权限失效与 owner 引用释放。
既有 Task/附件回归保持原来源、迟到结果及上下文 CAS 门禁。

真实专项继续使用原 SidePanel/RunPort/Composer 的创建、发送、导航、重连与确认丢弃，
新增真实 fork 在发送前和重连后再次 open 必须保留原 controller/草稿的严格断言。
DSH 是否可用仍以公开 runtime 能力为准，不能伪造 discard 或把不可用称为闭环成功。
日志、离屏自绘 PNG、程序与源码快照保留在
`.runtime-fixtures/sidechat-shared-destination-20261023/`；验证结果与身份见同名 JSON。

本轮不启动桌面窗口，不执行屏幕截图、签名安装或权限请求。
离屏自绘 PNG 不代表原生上屏、物理中文输入、阅读锚点或跨进程验收。
共享 target 复用 `/tmp/agentkit-layout-text-292-target`，没有新独立 target。

## 本轮结果

完整 UI workspace **623 项通过、1 项既有忽略**，包括 demo 宿主与第二公开 consumer；
严格 all-target Clippy、格式、UI 边界及 **74 项 Python** 检查通过。
Native Kernel 0.9.3（明确 UI-only）与 Pi 1.4.0 的原 SideChat 完整离屏路径通过，
`canonicalReopen` / `canonicalReopenAfterReconnect` 均为 true；DSH 0.2.1-rc.2
仍为原禁用门禁，`scratchLifecycleAccepted=false`，不声称其支持 SideChat。
Native 本轮未重跑完整 Kernel 生命周期/子历史协议；较早独立收据保持原身份。

固定程序 SHA-256：`656ff2392177118d7f2431540897ea600ed39c2c3b7175e2ba7b3be05dc76c6d`。
保留该 80,760,816 字节验收程序及源码/Cargo 锁快照、红/绿测试、协议与 consumer 日志、
离屏 PNG；没有删除缓存，实际释放 **0 字节**。没有安装/签名活动宿主。
`d1a0c09` 的 PR/push CI 均 headless 成功、UI 在严格 #53 阅读步骤失败；#48/#53 仍 OPEN。
