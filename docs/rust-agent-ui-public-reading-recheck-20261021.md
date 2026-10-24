# 公开 reading/reveal 依赖复核与分页修正

2026-10-21 重新读取 TekesUI 公开 branches、tags、#22/#28。两项 issue 仍 OPEN；新增 tag `0.2.0` 指向 `6d566a4774efba2c691bb8a596fc08cb2ecaf66a`，与上一轮 Scene3D main 相同。完整树没有 `crates/tekes-ui-controls/src/composition.rs`，也没有 `content_reading` / `content_reveal` 模块；匿名读取该 SHA 的 controls/lib.rs 结果一致。公开消费分支仍是8cf，无可升级的 reading/reveal 闭包。

默认依赖保持精确 `8cf0840790334ba224afc0595897c74231c712e6`，不修改 lock 或审计清单。原生 demo 的自动 reading attachment/update/item reveal 继续未接通，不把私有固定候选视为公开发布。上游缺口沿用 #22/#28，补充0.2.0核对结果，不创建重复 issue。

## 可独立完成的分页行为

对照 ClientKit `ClientSessionSidebar.swift` 的 `filterSessions` 和排序菜单，以及 Tekes `ClientAgentSidebarHostAdapter.swift` 的宿主边界：筛选/排序属于 AgentKit，基础控件仍只消费 TekesUI。

- 改变文件夹筛选或排序，将普通会话页恢复为5条；全局置顶和真实页外已选行仍保持原规则。
- 普通库存刷新、相同偏好发布、分组和置顶折叠不重置已展开页数。
- `Sidebar::reset_paging()` 供宿主处理重复选择当前筛选/排序。ComponentDemo 的重复筛选动作已调用它；未来排序菜单同值动作也必须调用它，不能将任意偏好刷新当作用户重选。
- 拒绝的 publication/不支持的排序保持原分页及控件；过期“显示更多”请求不能继续生效。
- 分页不请求历史，不自行刷新库存，不操作焦点/文本/滚动偏移，不触发发送。

独立公开 consumer 使用 `restart_sidebar_page` 调用公共 API；真实 ComponentDemo 测试检查改变和重选筛选时，已有中文 preedit、选区、外部 baseline、焦点和零提交保持。

## 证据边界

本轮结果见 `.runtime-fixtures/agentkit-public-reading-recheck-evidence/` 与公开版本审计子目录。组件 preedit 测试不代表物理输入法验收；自动 reveal、真实阅读锚点与物理中文输入法不得计为通过。未升级 SDK，也未修改 TekesUI、ClientKit、Tekes 或 Kernel。

最终默认公开依赖 UI 工作区 396 项通过，其中第二公开 consumer 42 项；fmt、all-targets 严格 Clippy、静态所有权边界、实际 Cargo 解析输入校验及 macOS 原生程序构建通过。本轮没有新原生窗口/物理 IME 验收，不继承旧二进制的原生收据作为本提交的运行证据。复用现有共享 target，未新建 target、未删除用途不明确的现用缓存，释放空间为0。
