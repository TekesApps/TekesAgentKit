# Sidebar 查询帮助与会话目录

本轮对齐当前 ClientKit `ClientAgentSidebarListView.observe` 的 `outline.rowHelp`：当前查询的 snippet 优先，缺少命中摘要时回退到原会话目录，不增加可见摘要行。ClientKit 与 Tekes 仅作为只读行为参考；未修改 SDK、宿主或 Kernel。

## 接入契约

- `SidebarRow.folder_path` 是原会话目录，由公开库存的 `cwd` 提供，宿主可通过 Primary Folder 适配器覆盖。不能拿当前工具执行目录 `workingDirectory` 或 session ID 代替；无目录时保持 `None`。完整 Primary Folder 目录管理、文件夹 outline 仍未实现。
- `SidebarInput.search_help: Option<SidebarSearchHelp>` 将查询来源和 snippet 映射与行成员关系放进同一个 publication。`SidebarPublication::project_search` 自动填充；未接查询的直接调用者传 `None`。
- `Sidebar::search_snippet` 仅在 loaded 且查询来源、revision、phase 与当前搜索输入完全相同时返回内容。新查询、清空、重连或代际切换会立即撤销旧命中。`row_help` 在没有当前摘要时回退到该行目录。空摘要保持原值，是否显示由 SDK 决定。
- 冲突 revision、无查询来源、跨端或不属于当前行集合的 snippet 在修改行树前拒绝。原会话按钮、动作及其退休规则沿用现有实现；第二 consumer 只通过公开组件/绑定 API 消费。

默认发布依赖仍为 TekesUI `8cf0840790334ba224afc0595897c74231c712e6`。它尚无 #35 声明，`sidebar_row_help_supported()` 返回 false，以上仅提供业务元数据，默认 demo 不显示自制 tooltip 或新增摘要行。UI 包仍为 `0.0.0`、`publish=false`，最新已发布无 UI 基线仍为 v0.3.8。

## 固定候选消费

仅在隔离 receiver 中消费 [TekesUI #35](https://github.com/TekesApps/TekesUI/issues/35) 固定包：

- identity：`3366559a1a524909f562fa2fdd1c8646d762b5ba0eb1294b5480c39d9e9c37ef`；503 个源文件，基于 a30 包变更13个路径。
- archive SHA256：`de2ee991aaa61912b8b6b03d183b244f6a7fd8884f8ed3b9448ce04e389c8ce2`。
- 在原行 `Content` 上调用 `set_help`，原 SDK 负责 dwell、位置、样式、clip、撤销和语义；原 TextBackend 只将只读 label shaping 委托给现有 CoreTextShaper。没有新建帮助节点、焦点目标、编辑器或 tooltip 引擎。
- 持续可见的同一行保留原 owner；查询进入 Searching 撤下结果行时，旧 owner/action 必须退休，结果返回后显式重新声明。元数据变化不能取消 Escape/click 的关闭状态；在同一 owner 内移动不算新手势，退出再进入才重新等待显示。

`scripts/test-reading-candidate.py --sidebar-help --append-padding-intent-probe` 在既有 Run/rail/focus/padding 探针基础上叠加四项实际 Sidebar/ComponentDemo 回归，并安装候选专用原生查询场景。其余必需参数沿用该脚本的显式固定输入，不从活动 SDK 工作树读取依赖。来源及命令见同名 JSON。

旧 e5b30ac0 包在交接前被上游撤回，本端未编译或提升；其原始输入保留。上游 Root 另一个身份的原生记录只作为 producer 证据，不计入本端验收。

## 原生窗口与测试边界

本端实际窗口依次验证原查询失败/原 Retry、库存恢复、查询 A→B、长中文/emoji/组合字符摘要、清空后目录回退、目录更新、原 owner 退休/保留，以及关闭状态不被更新解除。四张截图已逐一查看：原提示可见、长摘要按库布局换行、目录先后显示 `/fixture` 与 `/fixture-updated`。布局和样式均消费 SDK 默认值。

主 Composer 通过原 SDK 的 AppKit 测试入口建立合成 preedit。显示/更新帮助期间，文本、marked 选区、焦点及 baseline 保留；组字中的实际行点击未激活，显式 Unmark 后原按钮一次点击只激活一次，无提交。点击断言等待原生按下/抬起事件到达，不用固定等待时间冒充事件已执行。公开库存通知同时推进 Run revision，受控草稿确认可以推进 editor epoch；此处另核对文本/选区/marked/焦点不变，再以新 baseline 验证后续静态保持。纯帮助更新的 baseline 保留由独立组件探针验证。

原生关闭使用原 NSView `cancelOperation:` 回调，未宣称物理 Escape 验收。尝试 `Action::Key { key_code: 53 }` 时，SDK 的空字符合成事件经输入系统产生空 Commit 并删除选区；已提交 [TekesUI #37](https://github.com/TekesApps/TekesUI/issues/37)。失败日志、二进制、探针源和截图均保留。它证明该合成路径无效，尚不能归因于物理 Escape。后续 `cancelOperation:` 路径文本与选区保持。

本轮不是物理中文 IME、VoiceOver、SideChat 全生命周期、窗口最小化后真实激活、完整长文阅读/分页锚点或全页 ClientKit 体验验收。Native/Pi/DSH 的真实独立进程与公开查询/历史/组件回传复验属于另一个离屏层，不与此受控原生 fixture 混算。

## 交付状态

默认 UI workspace 364 项、固定候选 workspace 391 项（包含26项候选场景及原wrapper检查），其中第二公开 consumer 38 项；两者 all-target 严格 Clippy 和 native build 通过。Python 验证工具26项通过，默认解析到原公开pin的8包/199文件。最终默认二进制另通过 Native Kernel0.9.1、Pi1.3.0、DSH0.2.1-rc.2 的公开查询、库存刷新、历史与 Transcript 内容离屏回传；使用隔离本地provider，无付费模型调用。当前未修改无UI核心，183项无UI全套证据沿用上一已提交阶段，不冒称本轮重跑。

继续复用现有 target，未新建独立构建缓存。所有自有验证终态后，检查进程、取得排他Cargo锁，并用当前metadata/最终依赖文件及lsof核对，删除已被后续固定候选替代的c779原生v2/v4/v6中间依赖141个文件，释放243,707,904字节（约232.4 MiB分配空间）。这些阶段的验收可执行文件哈希不变；源码、固定快照、未提交改动、日志、收据、截图和现用/用途不明确的缓存全部保留。

逐项测试终态、精确来源、可执行文件及截图哈希、缓存清理结果记录在同名 JSON。默认 pin 未提升，PR #10 继续保持草稿。#36 原生选项/置顶移动菜单、Primary Folder/文件夹、多端聚合、本地 FTS、持久活动时钟，以及其余完整 Views 和最终验收仍在交付范围内；本轮不触发合并或 tag。
