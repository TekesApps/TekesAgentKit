# Sidebar provider 查询与目录成员关系交付（#22）

实现 `147642bf1e3a4f7b9ef60b13dff54b4ca8b6c5df`，PR [#10](https://github.com/TekesApps/TekesAgentKit/pull/10)。[设计及基准](rust-agent-ui-sidebar-query-membership-20261024.md)已先回填 [#22](https://github.com/TekesApps/TekesAgentKit/issues/22#issuecomment-6379453846)。两个新子会话分别实施 provider 投影与 folder/paging/navigation；主会话集成、修正并验证，另一新子会话独立审查。

当前 ClientKit 会话 Sidebar 的 pins 与普通会话共享 query/folder predicate。AgentKit 已移除 pinned 的 provider 查询无条件放行，并在目录筛选后才划分展示分区。匹配 pin 不重复，未匹配 pin 隐藏，清空恢复；筛选不修改 canonical pin 偏好、选择、草稿或持久展开状态。被目录排除的 selected pin 不生成伪可见目标，也不强行清除过滤器。

主会话进一步收紧了查询未完成的处理：Searching、Failed 或 unresolved 只显示待定/失败诊断，底层库存保留；不把未过滤库存呈现为新查询命中。`loaded` 仍表示库存加载。已修正单端空首页 `has_more=true` 的错误“没有匹配”提示，并要求 `results_ready` 只与 Complete 配对。未知 hit 随后由原 inventory publication 恢复的 ComponentDemo 集成分支仍保留，继续验证中文合成 preedit、焦点及草稿不变。

最终 UI workspace **677 passed / 0 failed / 1 既有忽略（TekesUI #49）**，96 组结果，使用 `--no-fail-fast` 完整执行。严格 all-target Clippy、普通 demo build、两个 workspace fmt、UI 边界和验收记录校验通过。独立外部 consumer 实际编译并运行，验证 pin 受 folder filter、清除恢复、pin 事实不丢，并挂载共享组件；workspace 第二 consumer 另验证 provider pending 与完整目录语义。SDK 仍为精确 `50ec49f3446924fdbc66b3cff22cd8cfe150fe7c`，243 份审查输入重新 hash 一致，没有新增依赖或修改 TekesUI/Kernel/Swift。

新回归包含跨端同 session ID 的 pin 命中隔离与去重、共同目录过滤、目录完整 catalog 不随展示过滤丢失、旧控件退休、filtered selection、待定/失败诊断、空首页仍有更多、错误 ready/snippet 原子拒绝和未知命中后续恢复。早期缺失测试 root、旧契约断言失败、审查前完整 suite 日志均保留；本结果依据 `query-membership-reviewed-workspace-tests.log`，不混用中间版本。

原生 pin-query/folder selftest 的源断言及未来收据字段已更新并编译，**未执行**。历史文档已标记旧“独立 pins 不过滤”规则被新契约取代，历史 JSON/程序/截图不改写为新语义验收。

精确日志 hash、源码清单、程序和范围见[机器收据](rust-agent-ui-sidebar-query-membership-results-20261024.json)。本地证据根 `.runtime-fixtures/sidebar-host-20261024/`，源码清单 `query-membership-source-manifest.json`；demo/consumer 分别归档 `agentkit-sidebar-membership-demo` 与 `agentkit-public-sidebar-membership-consumer`，共 99,291,264 bytes。

前一 head `9b1d263` 的 PR [40059408663](https://github.com/TekesApps/TekesAgentKit/actions/runs/40059408663) 已完成：headless、UI workspace/格式/Clippy、独立 consumer 成功，严格 nested detail reading 失败，后续 demo build 跳过。仅记录该 run steps，未取得本次 raw 阅读 JSON。新提交另看各自 CI，不合并/打稳定 tag，不豁免 TekesUI #53 的严格门槛。

**仍未完成**：本地名称 FTS、local/provider 组合、bm25 排名、远端 continuation、完整分页/reveal 与真实多 harness/桌面/物理中文 IME/AX/ProcessContainer。本批是 provider 成员关系和共同目录过滤修正，不是完整搜索 parity，不关闭 #22。后续内存派生索引的候选依赖只做了官方源码预审，尚未加入或编译。

全部 Cargo 复用共享 target，没有新建 target。只读审计未证明旧单测 feature 变体可删，继续保留；没有删除文件，实际释放 **0 bytes**。保留源码/WIP、固定依赖快照、历史证据及验收程序，未中断正在执行的验证。
