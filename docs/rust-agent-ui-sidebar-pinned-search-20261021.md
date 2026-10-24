# 搜索保留独立置顶区

> 历史阶段记录：其中“pins 不受 query/folder filter 影响”的解释已由 2026-10-24 的[当前会话 Sidebar 成员关系契约](rust-agent-ui-sidebar-query-membership-20261024.md)取代。原程序/测试/截图保持历史归属，不能证明新语义；新切片的本地名称 FTS、排名与真实窗口验收仍开放。

再次对照当前 ClientKit `ClientSessionSidebar.sessionRows` 发现，置顶会话来自 `globalThreadSnapshot.pinned`，普通会话来自搜索/过滤后的 `sessionThreads`。前者始终形成独立置顶区，不应被搜索命中集合过滤。此前 Rust `project_search` 同时过滤两类行，并用 `loaded=false` 表达查询等待，导致未命中置顶项消失，查询过程中原置顶按钮被卸载。本轮修正这一行为。

## 公开输入与行为

- `SidebarInput.loaded` 只表达原库存是否恢复，搜索进入 Searching 或等待未知 hit 的公开库存刷新不会将它改为 false。库存本身未恢复时仍不显示/激活行。
- `SidebarSearchHelp.results_ready` 显式表达普通搜索结果是否已与库存完成关联。它与查询来源、snippets、实际行成员关系处于同一 publication。直接构造实验输入的消费者需要填写该字段；`SidebarPublication::project_search` 自动生成。
- 置顶行始终来自原库存，按现有 pin 顺序展示；普通行仅在结果 ready 后按完整 SessionAddress 加入。命中的置顶会话不会在普通区重复。搜索等待时不把旧普通结果当作新查询结果，仍退休它们的旧 owner/action。
- 结果等待时 snippets 为空，原置顶会话的目录帮助可继续回退。未知 hit 尚未关联时显示“搜索结果尚未就绪”，即使置顶区非空也不会误报无命中。失败、完整无命中和清空查询均保持原置顶按钮；清空恢复完整库存。
- pending 但携带 snippets，或 Searching 却宣称 ready 的冲突输入在树修改前拒绝。查询来源的 revision/epoch/generation 检查不变。
- 绑定包公开重导出既有 `SessionSearchHit`/`SessionSearchResults`，第二消费者无需额外依赖内部契约包即可构造公开输入。没有新增查询状态机、pin 存储、网络 API 或基础 UI 控件。

默认 TekesUI pin 仍为 `8cf0840790334ba224afc0595897c74231c712e6`；#35 私有336包的原 `set_help` 声明只在 ready 且来源匹配时取摘要，否则回退原目录。没有提升依赖或修改 SDK。此前帮助接入记录的具体源码、测试数和原生产物作为历史阶段保留，本轮验证单独记录。

## 验证

定向测试覆盖完整命中、Searching、失败、无命中、未知 hit 等待/恢复、库存未恢复、跨端及原子输入拒绝。实际 ComponentDemo 在这些变化前建立合成 preedit，核对原置顶 Button 地址、中文文本、marked 选区、baseline、焦点和导航选择保留，无提交。第二公开 consumer 独立验证“未知普通结果 + 原置顶区”的语义与目录回退。

原生 fixture 经原公开搜索失败、实际 Retry、库存恢复和清空，再通过共享 pin 动作置顶 a/b。新查询只有 b 命中时 a 保留，b 不重复；Searching 已观察到两项原 pin，随后完整无命中仍保留两项；清空恢复库存。原 a Button 实例、选择和中文草稿保留，无提交。两张实际截图已查看，原置顶区及其两项均可见。此场景仅使用 SDK 原控件、截图和定时唤醒入口，不复制输入、布局或渲染。

完整测试终态、依赖来源、三端回传、原生二进制及截图哈希和清理记录见同名 JSON。工具驱动原生 fixture 不等于物理 IME、VoiceOver、三个 harness 的原生阅读/分页或全页 ClientKit 体验验收。完整 UI 目标仍未完成，PR #10 保持草稿。

本轮最终默认 workspace 367 项、固定336候选 workspace 394 项，其中第二公开 consumer 39 项；两边 all-target 严格 Clippy 通过。固定候选生成器保留原26项 Run/rail/Start/focus/padding/help 回归及原wrapper检查，五步重放全部通过。Python工具26项通过，fmt、静态所有权及默认8包/199文件精确依赖检查通过。最终默认二进制与原生验收二进制哈希相同，另复验 Native Kernel0.9.1、Pi1.3.0、DSH0.2.1-rc.2 的独立进程公开查询与内容离屏回传，三端通过。未修改无UI核心，未重复执行其183项上一阶段全套。

继续复用既有 target，无新独立缓存。在所有自有验证终态后，检查进程、排他Cargo锁、lsof及当前metadata/原生依赖文件，删除被本轮替代的两个旧336 receiver快照的112个纯中间编译文件，释放210,800,640字节（约201.0 MiB分配空间）。原/新验收可执行文件哈希不变；固定源码、失败/成功记录、截图、WIP、现用及用途不明确的缓存保留。
