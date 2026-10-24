# Issue #23 SidePanel 定向审计（2026-10-24）

## 范围与结论

首次审计基线：AgentKit `52288a08bf77b9071547497992e5c20294f706b4`。续审基线：`ddc49c75bcd2cf4be4dd17152db7d595e6ed65da`。范围为 Task/子代理 SidePanel 的 `loading / missing / error / ready`、Task 权威 session pointer 变化、父导航 A→B→A、关闭和迟到结果。上轮静态审计没有发现足以支持产品代码修复的确定缺陷；本轮只新增两项针对具体验证缺口的回归测试，未改组件实现。没有运行 Cargo、启动 harness 或桌面程序。

现有 Rust 回归针对独立行为和竞态编写，没有试图穷举所有状态、导航和异步完成的笛卡尔积。下表区分已有覆盖与尚未跑过或没有直接断言的范围；这些组合边界是验收范围记录，不单独构成产品缺陷。测试名指向源码中的实际测试；本轮新增测试未执行。

## Task 状态与 pointer

| 场景 | 实现与可观察行为 | AgentKit 现有测试 | 仍未组合验收 |
| --- | --- | --- | --- |
| loading | `TaskPanelState::begin` 清空旧投影、令 `resolved=false`；`SidePanelTab::task_from_state` 保留 task ID 稳定身份但不提供 `task_scope`；SidePanel 展示 loading label 且不给正文 token。 | `ui/demo/tests/task_binding.rs`：`loading_missing_error_foreign_and_cancel_never_fall_back_to_parent_run`；`ui/demo/tests/side_panel_component.rs`：`task_identity_is_stable_while_shared_resolution_retires_changed_or_missing_session_content`。 | loading 状态与父导航、关闭、再次打开的完整异步交错没有一个单测组合覆盖。 |
| missing（确认无任务） | `commit(Ok(None))` 置 `resolved=true`、无 value；占位文案为“任务 Run 暂不可用”，不会回退到父 Run。 | `ui/demo/tests/task_binding.rs`：`loading_missing_error_foreign_and_cancel_never_fall_back_to_parent_run`；`ui/demo/tests/side_panel_component.rs`：`task_identity_is_stable_while_shared_resolution_retires_changed_or_missing_session_content`。 | 未把 missing 与跨父 A→B→A、迟到结果同时组合。 |
| error（读取失败） | `TaskPanelState::commit(Err)` 置 `error`、清空 value、保持 `resolved=false`；SidePanel 优先展示该错误文本，无 Run token；错误不会伪装为确认 missing。 | `ui/demo/tests/task_binding.rs`：`loading_missing_error_foreign_and_cancel_never_fall_back_to_parent_run`；新增 `ui/demo/tests/side_panel_component.rs`：`task_status_error_and_missing_retire_ready_content_and_stale_mount_tokens`。 | 新增用例尚未由主会话运行；也未与父关闭/重选组合。 |
| ready（同 pointer 更新） | Task ID 是 tab identity；同 scope 的 name/status 更新保留相同 Run 和草稿。 | `ui/demo/tests/task_binding.rs`：`task_status_updates_keep_exact_composer_but_pointer_aba_retires_callbacks`。 | UI/native 布局和完整窗口行为未验收。 |
| ready（pointer 替换） | tab ID 不变，但新 `task_scope` 触发 SidePanel mount epoch 更新并撤销旧 token；`TaskPaneBinding` retire 旧 Run 与回调。加载 token 也阻止旧 pointer 回写。 | `ui/demo/tests/task_binding.rs`：`task_status_updates_keep_exact_composer_but_pointer_aba_retires_callbacks`、`changed_task_pointer_during_host_preparation_cannot_admit_the_old_click`；`ui/demo/tests/side_panel_component.rs`：`task_identity_is_stable_while_shared_resolution_retires_changed_or_missing_session_content`。 | 多 parent 窗口中的 UI/layout 实际呈现仍未验收。 |

ClientKit 对照：`/Users/frankqin/dev/TekesClientKit/Sources/Composer/Agent/TaskRunPanelProjectionState.swift` 同样以 task ID 做投影身份并拒绝过期 load；`ClientAgentTaskView.swift` 在未解析时显示 loading，解析无 task 后显示 `Task run unavailable`，并在 Run document ID 变化时替换嵌入 Run。当前 ClientKit 加载器以 optional 返回值表达结果，`try? await store.task(...)` 将读取错误折叠为 `nil`，所以 UI 最终显示 unavailable。AgentKit 保留 `TaskPanelState.error` 并显示原始错误文本。这是更细的诊断映射差异，不代表错误被当成有效 Task，也没有据此改写成 ClientKit 的丢错行为。当前 ClientKit checkout 下未找到这些 projection 行为对应的测试名，因此本审计只把它作为实现对照，不声称已有 ClientKit 测试验收。

## 子代理状态、读取与授权

| 场景 | 实现与可观察行为 | AgentKit 现有测试 | 仍未组合验收 |
| --- | --- | --- | --- |
| loading / 正在打开 | `begin_child_open` 生成带 revision 的请求；请求尚未成功前不安装子代理正文。SidePanel 的 parent projection 未就绪时撤销所有正文 token。 | `ui/demo/tests/side_panel_component.rs`：`awaiting_projection_retires_all_content_without_empty_or_stale_body_and_never_revives_old_tokens`；`ui/demo/tests/panel_navigation.rs`：`late_child_open_cannot_replace_a_newer_owner_of_the_same_tab`。 | 独立的可见“子代理 loading”状态、布局和完整窗口渲染没有作为本次目标验证；没有将加载态与全部父切换排列组合。 |
| missing / 父 pane 已撤销 | child 继承 direct parent 及祖先 authority；parent retired 时 child access 变为 Missing，不能再从旧 pane 获得控制能力。成功打开但 child 历史为空，与 missing parent 是不同状态。 | `ui/demo/tests/child_binding.rs`：`nested_child_inherits_retirement_of_its_original_parent_pane`、`task_to_another_scope_inherits_its_original_child_parent_authority`；`ui/demo/tests/panel_navigation.rs`：`closing_a_subagent_returns_to_the_last_task_without_retiring_its_binding`（该测试断言 child binding access 被撤销，父 Task 仍有效）。 | 子代理缺少统一显式 missing 空态；父撤销后的真实控件呈现没有组合验收。 |
| error / 打开或读取失败 | `ChildOpened(Err)` 仅在原请求仍 current 时发 notice，不安装 source；读取错误写入 `pane.read_error`，经 `ReadOnlyTranscriptInput` 传到只读正文。分页仍由用户明确重试，不自动刷新。 | `ui/demo/tests/child_binding.rs`：`builtin_bounded_history_pages_only_child_content_and_mounts_without_a_composer` 覆盖分页读失败后不自动重试、显式再次操作可继续；`late_child_open_caches_original_parent_without_stealing_selection_and_owner_drop_revokes_callbacks`；新增 `late_child_read_error_cannot_pollute_a_replacement_source` 覆盖旧只读 source 的错误不覆盖替换后的同目标 source，并确认当前 source 错误仍可见。 | child-open `HostResult::ChildOpened(Err)` 的错误 notice dispatch 未被此测试覆盖；该分支位于本任务禁止修改的 `ui/demo/src/components_native.rs`。 |
| ready / 独立子 Run | `ChildPaneBinding::open` 固定 direct parent、child ID 与 layout；独立 session 可显示可读历史，但不因此获得主 Run 发送控件。 | `ui/demo/tests/child_binding.rs`：`nested_child_uses_direct_parent_and_existing_catalog_policy_without_changing_readable_history`、`late_child_open_caches_original_parent_without_stealing_selection_and_owner_drop_revokes_callbacks`。 | Native/Pi/DSH 三端的真实窗口和完整 SidePanel chrome 未验收。 |
| ready / 同 session 有界历史 | 只读 transcript 依赖 source、scope、publication、connection epoch 和 source-ready 检查；内建分页受显式 intent 与 epoch fencing 约束，不挂 Composer。 | `ui/demo/tests/child_binding.rs`：`builtin_bounded_history_pages_only_child_content_and_mounts_without_a_composer`；`ui/demo/tests/side_panel_component.rs`：`nested_child_parent_is_content_identity_and_foreign_parents_cannot_replace_it`。 | 这些是现有 headless/component 测试，不等同三端单独的完整 UI/native 验收。 |

## 父导航、关闭与迟到结论

| 组合 | 已核验行为 | AgentKit 现有测试 | 尚缺 |
| --- | --- | --- | --- |
| Task pointer A→B→A | generation token 和 Run ownership 都是单调生命周期；旧 A 的回调、prepared send、草稿权限不会因回到业务 pointer A 而复活。 | `ui/demo/tests/task_binding.rs`：`task_status_updates_keep_exact_composer_but_pointer_aba_retires_callbacks`、`changed_task_pointer_during_host_preparation_cannot_admit_the_old_click`。 | 没有把 SidePanel mount 的所有 ready/loading 状态与此 pointer ABA 放入同一测试。 |
| 父导航 A→B，A 的 Task 读取迟到 | 迟到结果可以缓存回原 parent，但不得抢走当前 B，也不应破坏 B 的焦点/草稿；显式选回 A 后恢复其 Task。 | `ui/demo/tests/panel_navigation.rs`：`original_task_completion_can_cache_after_parent_switch_without_navigating`。 | 尚未运行 Task pointer 与父级导航交错的新组合；现有独立竞态测试仍按其覆盖范围解释。 |
| 关闭 Task / 关闭 Subagent | Task 关闭选回最后一个 tab；子代理关闭回到前一任务 tab，并撤销 child pane 的访问。 | `ui/demo/tests/panel_navigation.rs`：`closing_a_task_selects_the_last_remaining_open_tab`、`closing_a_subagent_returns_to_the_last_task_without_retiring_its_binding`。 | 关闭与所有状态、父级重选、异步操作的笛卡尔组合未覆盖。 |
| 关闭或新 owner 后旧 child 结果迟到 | close tombstone 阻止尚未安装的较早 open 重新出现；更晚 owner 获准后，较早请求不能替换它。 | `ui/demo/tests/panel_navigation.rs`：`closing_a_child_fences_a_never_installed_older_open_result`、`late_child_open_cannot_replace_a_newer_owner_of_the_same_tab`。`ui/demo/tests/child_binding.rs` 新增 `late_child_read_error_cannot_pollute_a_replacement_source` 覆盖已安装 source 替换后的迟到读取失败。 | stale open 错误的 host notice 分发仍待 #22 owner 处理/验证；跨 endpoint 及容器 transport 不在本次范围。 |
| Task 父级在 child 历史读取中退休 | 祖先 authority 再次检查阻止 child open 装入已失效 pane。 | `ui/demo/tests/task_binding.rs`：`child_open_cannot_install_after_its_task_parent_retires_during_history_baseline`；`ui/demo/tests/child_binding.rs`：`nested_child_inherits_retirement_of_its_original_parent_pane`。 | 未与 Task error/missing 呈现和 UI close 同时组合。 |

UI mount 自身另有 token fencing：`ui/demo/tests/side_panel_component.rs` 的 `removed_and_reopened_tabs_and_evicted_parent_views_reject_old_mounts` 验证关闭重开 tab 和 parent view eviction 后旧 content token 无法安装；`awaiting_projection_retires_all_content_without_empty_or_stale_body_and_never_revives_old_tokens` 验证 projection 暂不可用时不遗留旧正文。

## 给主会话的最小验证集

下面是本审计建议主会话集中运行的最小既有 Rust suite；当前子会话没有执行 Cargo：

```sh
cargo test --manifest-path ui/Cargo.toml -p tekes-agent-ui-demo --test task_binding
cargo test --manifest-path ui/Cargo.toml -p tekes-agent-ui-demo --test child_binding
cargo test --manifest-path ui/Cargo.toml -p tekes-agent-ui-demo --test side_panel_component
cargo test --manifest-path ui/Cargo.toml -p tekes-agent-ui-demo --test panel_navigation
```

通过以上 headless/component suite 只能证明对应 Rust 状态和回调边界；不能声明三端真实 harness、全窗口布局、原生输入、辅助功能或完整 ClientKit parity 已验收。原生桌面仍未运行。

## 主会话本批运行结果

主会话先前完整 UI workspace 回归通过：634 passed、0 failed、1 项原有忽略（TekesUI #49）。当时 demo package 的 `task_binding` 10、`child_binding` 5、`side_panel_component` 11、`panel_navigation` 15 项通过（41 项）；同名 consumer 测试文件另计。日志 `.runtime-fixtures/parallel-issues-20261024/ui-tests-complete.log` 保留。该运行早于本轮新增两项测试，不能当作它们的执行收据。本轮对两个改动测试文件运行 `rustfmt --check` 与 `git diff --check`；Cargo、桌面及 native `ChildOpened(Err)` notice 分发验证待主会话。以上仍是组件/夹具证据；没有真实窗口或系统 IME 验收声明。

## 续批主会话已运行

新增两项已实际通过：`child_binding` 6 项、`side_panel_component` 12 项，共 18 passed / 0 failed。日志 `.runtime-fixtures/sidebar-host-20261024/ui-targeted-tests-pass2.log` 中这两个 suite 成功；该命令最后另一个 Sidebar suite 的失败属于当时测试 fixture/动作入口问题，不能把整条命令写成通过。Sidebar 后续修正与完整 UI workspace 收据由主会话统一记录。没有运行桌面、真实窗口/IME，也没有新增 native ChildOpened(Err) notice dispatch 证据。

本批完整 UI workspace 随后通过 637/0/1 原有忽略，包含本文件新增测试，日志 `.runtime-fixtures/sidebar-host-20261024/ui-workspace-tests.log`；后续 #22 publication 可变性收紧不修改这些测试与 SidePanel 实现。
