# #22 / #23 / #25 非桌面续批结果

源码冻结于 `6b28136974013ef688d034b3583a40524442cfcb`，PR [#10](https://github.com/TekesApps/TekesAgentKit/pull/10) 保持试验状态。主会话协调和集成；独立子会话分别负责 SidePanel、Summary、组件 readiness、CI 和来源审查；独立 advisor 确认 SQLite 必须位于 UI bindings。没有修改 TekesUI、ClientKit、Kernel 或 Swift。

## 实现

- #22 `051d7f0`：UI bindings 的真实内存 SQLite FTS5 索引、精确依赖和原 source/revision 围栏；demo 持续复用 projector，多端一次索引全库存、按完整地址与已知 Complete provider hits 合并。未知命中不挡其他已知命中，provider 失败/不支持保留本地名称结果和诊断。显式清除标题不恢复旧 summary，也不把未命名 ID 当名称。local/provider 就绪与有限空态分开，后到库存无需重发 query 可恢复。根 headless/presentation 没有 SQLite，原门禁不变；UI 仅允许审核后的精确 bundled 配置。
- #23 `6b28136`：[定向审查](rust-agent-ui-sidepanel-followup-20261024.md)未发现确定产品缺陷，未修改状态机；当前 demo 的 Task 10、child 6、SidePanel 12、panel navigation 13，共 41 项通过。native ChildOpened(Err) dispatch 的直接运行证据和完整三端/窗口组合仍缺。
- #25 `6420876`：[缺失用量修复](rust-agent-ui-summary-followup-20261024.md)保留六条不可用值，区别于已知零和整个 Summary 不可用；Summary 13 项组件测试通过。来源动作和正式 slots 未发现确定缺陷，未声称最终视觉/AX 已验收。
- 去重新增 [#29](https://github.com/TekesApps/TekesAgentKit/issues/29) 跟踪明确的 main/subagent/ephemeral 事实与 Sidebar/selected-read 一致性；缺字段不当作已确认 main，不修改 canonical 全局 child/history。

## 验证层级

UI workspace **693 passed / 0 failed / 1 既有忽略 #49**（97 result groups）；其中七项实际 FTS 测试、九项单端结果测试、七项多端结果测试，包含真实共享库存 fixture 驱动的双端 demo 本地名称查询。完整 suite 后只调整测试模块的位置以满足严格 Clippy，最终组件 lib 12 项重跑通过；没有再修改生产行为。presentation **241/0/0**，Python **92** 通过。root/UI workspace 全 targets 严格 Clippy、格式、headless/UI/pin 边界和验收记录检查通过。正常 demo 编译、独立 public API consumer 编译运行通过；后者只证明组件公开 API，不能扩大为完整 FTS、真实 harness 或窗口证明。

精确 SDK pin 保持 `50ec49f3446924fdbc66b3cff22cd8cfe150fe7c`，核验 243 文件。新程序已单独归档并记录 hash/动态库清单；没有启动原生 demo、截图、权限、签名安装或桌面验收。

严格 nested reading 在本次本地同进程无窗口程序仍 **failed**：inner offset 100、父追加后 offset 634、父 following 仍 true、detailVisibleAfterAppend=false。跟踪 TekesUI #53，不豁免门禁。77a9ed8 的 CI headless/UI tests/Clippy/独立 consumer 已通过，只有此严格步骤失败、后续 demo build 跳过；未取得该 CI run raw JSON，不能把本地数值冒充 CI 观测。新推送必须看自己的 CI。

仍开放：bm25 排名、远端 cursor、Native/DSH 分类公开事实、真实多 harness、完整像素 reveal、桌面/物理中文 IME/AX/ProcessContainer 与性能。不关闭宽范围 Issue、不合并或打 tag。

## 磁盘与证据

继续复用 `/tmp/agentkit-layout-text-292-target`，没有新建 target。验证进程结束后，核对旧 fingerprint 依赖及占用，持单一 Cargo flock 删除四个仅属于未提交旧 presentation+SQLite 落点的 `.rlib/.rmeta`：`debug/deps/libtekes_agent_presentation-{808cf6d3fe4e8d43,a61fef1e203f1002}.{rlib,rmeta}`。实际删除分配块及 deps 的 du 减少均为 **39,043,072 字节（37.25 MiB）**；APFS 共享和其他任务会影响卷空闲值，不将卷变化归因成本次物理回收量。

源码/WIP、固定快照、全部历史验收证据、当前与旧验收程序、其余归属不明缓存均保留，WIP 与程序 hash 复核通过。没有执行旧的约 10.8 GiB 预计清单，也没有整删共享 target。精确日志、源清单与 hash、保留程序、单锁清理收据见[机器收据](rust-agent-ui-name-fts-results-20261024.json)及 `.runtime-fixtures/sidebar-host-20261024/name-fts-*`。
