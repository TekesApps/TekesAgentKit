# Issue #23 SidePanel 状态路径续审（2026-10-24）

## 结论与基线

按主会话分工续审 Task/子代理 pointer、父关闭、A→B→A、只读有界 pane 与异步迟到路径。AgentKit 检查基线为 `77a9ed8d7c7dfe86b43679a16e94167c7b8ebf6f`；ClientKit 只读基线为 `c289bc9d33edcd83a782ac17927a8c65a027eccd`，Tekes 宿主只读基线为 `8858ccd9d46590852ec7a0949aee289fc6798d70`。未修改实现代码；未运行 Cargo、桌面、真实 harness 或截图。

本次没有发现足以支持产品代码修改的确定缺陷。既有 `docs/rust-agent-ui-sidepanel-issue23-audit-20261024.md` 记录的测试、运行收据和未覆盖项仍有效；本续审补充直接实现路径和边界，没有把旧测试收据升级为本轮执行结果。

## 实际状态路径

| 路径 | 当前实现 | 防迟到/隔离行为 | 对应现有回归 |
| --- | --- | --- | --- |
| Task loading、missing、error | `TaskPaneBinding::begin` 生成 `TaskLoadToken` 并 retire 旧 Run；`TaskPanelState::commit` 只接受当前 token，错误与确认缺失分开保存。组件从 state 构造 Task tab；没有 resolved Run 时不提供 Run pane。 | token 包含 parent scope、task ID 和递增 generation；取消会清除当前 token。迟到值不能因 task ID 或 pointer 返回 A 而重获资格。 | `task_binding.rs`: `task_status_updates_keep_exact_composer_but_pointer_aba_retires_callbacks`、`loading_missing_error_foreign_and_cancel_never_fall_back_to_parent_run`；`side_panel_component.rs`: `task_identity_is_stable_while_shared_resolution_retires_changed_or_missing_session_content`、`task_status_error_and_missing_retire_ready_content_and_stale_mount_tokens`。 |
| Task pointer 替换与同 pointer 更新 | `TaskPaneBinding::commit` 比较前后 scope；scope 变化时递增 Run source generation 并释放旧 Run。name/status 更新保留相同 Run 和草稿。`open_resolved` 在 baseline await 前后复核 generation、当前 pointer 与 parent authority。 | 旧 baseline 的成功与失败都不能覆盖新 Task pane；TaskRunGuard 还核对 owner、generation、scope 和 resolved state。 | `task_binding.rs`: 上述 A→B→A 用例、`changed_task_pointer_during_host_preparation_cannot_admit_the_old_click`、`child_open_cannot_install_after_its_task_parent_retires_during_history_baseline`。 |
| Task tab 关闭 | SidePanel close 先更新本地导航；随后对已打开的 `TaskPaneBinding` 调用 `cancel` 并移除 pane Run 与本地 attempt。 | 取消使当前 load token 失效，撤销已捕获 Run callbacks；这段路径没有对远端 Agent task 发 cancel RPC。关闭 Task tab 属于本地导航操作。 | `panel_navigation.rs`: `closing_a_task_selects_the_last_remaining_open_tab`；`task_binding.rs`: cancel/guard 断言包含在 A→B→A 与 loading/missing/error 覆盖中。关闭与所有 Task 状态的组合没有穷举。 |
| 子代理打开和只读有界历史 | `ChildPaneBinding::for_run` 在读取前后检查 direct parent authority，并把祖先 authority 传入 child Run。SameSession 才能构造只读 child source；内建分页需显式 intent、同 source/scope、已发布帧和当前 connection epoch，且不挂 Composer。自定义 history source 不会误显示内建分页按钮。 | `retire` 撤销 child owner；父 pane 缺失会让 child `PaneAccess` 变 Missing。分页 intent 先 claim，再复核来源与 epoch；断连只会降低当前页操作资格，显式 refresh 才能建立新 epoch。 | `child_binding.rs`: `builtin_bounded_history_pages_only_child_content_and_mounts_without_a_composer`、`nested_child_inherits_retirement_of_its_original_parent_pane`、`late_child_read_error_cannot_pollute_a_replacement_source`；`readonly_child.rs` 覆盖刷新、分页和帧来源门禁。 |
| 父导航、关闭与 child-open 迟到 | `PanelRequest` 绑定父 scope、主导航 revision、SidePanel revision、父 projection generation。安装 child source 前校验请求仍 current；同 child ID 的旧 owner revision 不能替换较新 owner。关闭 child 会递增 revision 并留下 tombstone，然后 retire pane。 | A→B→A 中 revision 单调递增；即使业务 scope 返回 A，旧请求仍然失效。子代理 tab 关闭只撤销本地 pane，不取消 child 远端运行。 | `panel_navigation.rs`: `closing_a_child_fences_a_never_installed_older_open_result`、`late_child_open_cannot_replace_a_newer_owner_of_the_same_tab`、`closing_a_subagent_returns_to_the_last_task_without_retiring_its_binding`；`child_binding.rs`: late-open/owner-drop 和迟到 read error 用例。 |
| native child-open 错误回调 | `HostResult::ChildOpened(Err)` 只有在 `child_request_current` 成立时才显示 notice；成功结果仍经 `install_child_source` 做 scope、revision、owner 与 snapshot 校验。 | 父已切换、关闭或新导航发生后，旧错误不污染当前 pane 的 notice；错误成功路径均不自动重试。 | 当前没有直接针对 `ChildOpened(Err)` notice dispatch 的回归。本次只静态核对 `ui/demo/src/components_native.rs` 分支；完整 host dispatch 运行仍由主会话验收。 |

## ClientKit 与 Tekes 对照

ClientKit `TaskRunPanelProjectionState` 以 task ID 做 tab identity，并用 expected/current task ID 与 cancellation 检查拒绝旧结果。`ClientAgentTaskView` 依据最新 task row 的 session ID 替换嵌入 Run；未 resolved 显示 loading，已 resolved 且无 task 显示 unavailable。其 store read 用 `try?` 把读取错误映射成 unavailable；AgentKit 的 `TaskPanelState` 保留错误文本，提供更细诊断，仍不会把错误当成有效 Task。

Tekes 的 `SidePanelHostView` 委托 ClientKit `ClientSidePanelHostView`、Task view 和导航模型，产品层提供 tabs/select/close/open 回调。AgentKit 的 Rust demo 由 `SidePanelIntent` 返回导航意图，host adapter/`ComponentDemo` 采纳并持有导航 revision；两边都没有把远端取消绑定到关闭 Task/child tab。本次未在 Tekes 宿主或 Swift 文件中改动。

## 尚未证明的范围与主会话验证建议

未覆盖内容仍包括完整窗口/chrome 的各状态组合、三端真实 harness 对同一 Task pointer 与家庭导航的可追溯来源、Task 状态与 parent A→B→A 的全交错组合、native `ChildOpened(Err)` notice dispatch 回归，以及物理 IME/焦点/辅助功能。现有测试是状态与回调层证据，不代表这些验收已完成。

本子会话未执行验证。主会话如需本轮复核，建议串行运行现有最小 Rust suite：

```sh
cargo test --manifest-path ui/Cargo.toml -p tekes-agent-ui-demo --test task_binding
cargo test --manifest-path ui/Cargo.toml -p tekes-agent-ui-demo --test child_binding
cargo test --manifest-path ui/Cargo.toml -p tekes-agent-ui-demo --test side_panel_component
cargo test --manifest-path ui/Cargo.toml -p tekes-agent-ui-demo --test panel_navigation
```

如需补齐可自动验证的窄缺口，优先为 native `ChildOpened(Err)` 加一个 current request 显示 notice、已过期 request 不显示 notice 的 host dispatch 回归；本次没有因单纯缺少该用例就改生产分支。PR #10 的合并、Issue 回填和状态由主会话统一处理。
