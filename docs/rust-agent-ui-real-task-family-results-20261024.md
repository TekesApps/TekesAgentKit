# #23：真实三端家族目录与原子页分发（2026-10-24）

[设计](rust-agent-ui-real-task-family-contract-20261024.md) · [机器收据](rust-agent-ui-real-task-family-results-20261024.json) · [PR #10](https://github.com/TekesApps/TekesAgentKit/pull/10)

在同一当前离屏 demo 中，Native 0.9.3、Pi 1.3.0、DSH 0.2.1-rc.2 各两个真实 parent/child 的家族路径通过。仅使用隔离 home/data、公开 SEP API 和 loopback 合成 provider；无付费模型、私有历史写入或桌面交互。此前来源预检只是来源证明，本轮补的是原控件→原命令→原结果分发→原绑定→真实 ContentHost 节点。

## 实现与对齐边界

- 新增 AgentKit `FamilyCatalog` 高层控件，消费固定 TekesUI `50ec49f3446924fdbc66b3cff22cd8cfe150fe7c` 的现有 Stack/Text/Button/preserving_focus。没有修改 SDK、Kernel、ClientKit 或 Tekes 源码，也没有新增 Swift。
- `FamilyCatalogBinding` 复用已有 `ChildCatalogState` 的 begin/commit；仅显式读取公开 catalog。原父 Run 采用弱引用；epoch、generation、publication 和不透明 owner key 共同约束回调。绑定重建即使地址/计数/数据相同，也不能复活旧控件捕获。
- 当前 ClientKit `Sources/Composer/Agent/SummaryProviders.swift` 仅为独立 session Task 提供 OpenTask；Tekes `ClientAgentSummaryHostAdapter` 转发到原 navigation。Native 父 scope Task 因而保持实际挂载的普通 Summary 行，没有强加 OpenTask；bounded child 从明确的关联目录进入原 SameSession 只读 Transcript。Pi/DSH 使用原 IndependentSessions child binding，目录不授予发送权。
- Host 的 native draw 与离屏 pump 采用同一 `apply_result`。Window 仅提供原生通知/剪贴板能力，离屏分发不创建另一个 pane 状态机。目录失败由原 source 持有，旧读取不会作为当前 pane 的全局错误发布。

## 实际验证

六组独立来源逐一通过：原 Sidebar parent 选择、公开目录读取、原按钮 OpenChild、真实 child scope/非空历史、实际正文节点挂载、原 Summary/child tab 返回与原关闭。Native 两组均确认没有 Composer，canonical Task 行实际挂载且无独立打开 action。父会话 A→B→A 后，原 canonical 草稿仍在。

受控延迟明确分成两类：父导航改变后的真实 OpenChild 结果允许按原契约缓存原 parent，但不得抢走当前选择；原 child tab 关闭后才投递的真实 OpenChild 结果必须被关闭记录拒绝，不挂回内容、不污染当前错误。这是本地受控投递，不声称网络发生真实乱序。

关闭场景的 CPU 回归先复现 `Protocol("child source retired")`。修复是将原 `child_open_revisions` 关闭/替换记录检查提前到 retired-source 验证之前；红、绿日志均保留，没有删除原 authority 校验。

- UI 工作区：734 passed / 0 failed / 1 既有忽略（#49）。
- Python：161 passed，含 manifest 拒绝与家族收据的负向标准。
- 严格 Clippy all-targets、普通 demo 构建通过。
- 独立公开 API consumer 实际执行通过，六个独立挂载包含新的 FamilyCatalog；prepared receipt 的 `cargo_invoked=false` 仅描述准备步骤，不代表后续没有执行。
- 新功能外的原真实三端 Sidebar 路径另行复核，收据存放在下面证据目录。

## 证据与磁盘

本地证据根：`/Volumes/HP P900/dev/TekesAgentKit/.runtime-fixtures/real-multi-sidebar-20261024/`。

`family-host-verified/` 保留六组原始公开命令/返回、manifest、完整同进程 delivery、实际程序散列与离屏 PNG；`family-final-source/`、`family-final-source-identity.json`、`family-final-cargo-checks.json`、`family-final-postcheck.json` 将实际源码、程序、检查与归档关联。`family-retired-open-red.log` 和 `family-source-and-delivery-tests.log` 保留关闭错误的失败与修复。`family-host-attempt1/` 是早期库存未就绪失败；`family-host-attempt2/` 仅达到较早标准，不代替最终加强后的验证。原 Sidebar 复核为 `family-sidebar-regression/`。

继续复用 SSD `shared-target`，不新建 target；本轮未删缓存，实际释放 0 字节。历史失败、固定来源快照、原始协议日志和验收程序均保留；归档程序只归档，不作为新的原生宿主启动。

## 尚未验收

同一真实 Task row 动态更换 pointer 的公开 producer 尚未找到，本轮家族 A→B→A 不能替代它。真实 native window/chrome、物理 IME、OS AX、桌面可见性与 SDK #53 严格阅读仍未通过；#23 保持 open。TekesUI main 本轮再次核对仍为 `7c13ec0`，#53 OPEN，没有切换到 sibling WIP。PR 保持实验草稿，不合并、打 tag 或发布；新提交 CI 必须按新 head 单独核对。
