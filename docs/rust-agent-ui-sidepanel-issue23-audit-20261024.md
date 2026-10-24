# Issue #23 SidePanel 定向审计（2026-10-24）

## 范围与结论

审计基线：AgentKit `52288a08bf77b9071547497992e5c20294f706b4`。本次只检查 Task/子代理 SidePanel 的 `loading / missing / error / ready`、Task 权威 session pointer 变化、父导航 A→B→A、关闭和迟到结果。代码与测试是静态核对；没有运行 Cargo、启动 harness 或桌面程序。**没有发现足以支持定向修复的确定缺陷**，也没有改 Rust 代码。

现有 Rust 回归分散覆盖这些路径，但状态和导航组合还没有完整组合验收。尤其没有一条用例同时覆盖 Task 与子代理的四态、父 A→B→A、关闭和异步迟到。下面区分已被具体测试覆盖的行为与仍待组合验证的部分。测试名指向源码中现有测试；本轮未执行。

## Task 状态与 pointer

| 场景 | 实现与可观察行为 | AgentKit 现有测试 | 仍未组合验收 |
| --- | --- | --- | --- |
| loading | `TaskPanelState::begin` 清空旧投影、令 `resolved=false`；`SidePanelTab::task_from_state` 保留 task ID 稳定身份但不提供 `task_scope`；SidePanel 展示 loading label 且不给正文 token。 | `ui/demo/tests/task_binding.rs`：`loading_missing_error_foreign_and_cancel_never_fall_back_to_parent_run`；`ui/demo/tests/side_panel_component.rs`：`task_identity_is_stable_while_shared_resolution_retires_changed_or_missing_session_content`。 | loading 状态与父导航、关闭、再次打开的完整异步交错没有一个单测组合覆盖。 |
| missing（确认无任务） | `commit(Ok(None))` 置 `resolved=true`、无 value；占位文案为“任务 Run 暂不可用”，不会回退到父 Run。 | `ui/demo/tests/task_binding.rs`：`loading_missing_error_foreign_and_cancel_never_fall_back_to_parent_run`；`ui/demo/tests/side_panel_component.rs`：`task_identity_is_stable_while_shared_resolution_retires_changed_or_missing_session_content`。 | 未把 missing 与跨父 A→B→A、迟到结果同时组合。 |
| error（读取失败） | `TaskPanelState::commit(Err)` 置 `error`、清空 value、保持 `resolved=false`；SidePanel 优先展示该错误文本，无 Run token；错误不会伪装为确认 missing。 | `ui/demo/tests/task_binding.rs`：`loading_missing_error_foreign_and_cancel_never_fall_back_to_parent_run`。 | `side_panel_component` 有明确 error 文案的端到端组件断言尚无专测；未和父关闭/重选组合。 |
| ready（同 pointer 更新） | Task ID 是 tab identity；同 scope 的 name/status 更新保留相同 Run 和草稿。 | `ui/demo/tests/task_binding.rs`：`task_status_updates_keep_exact_composer_but_pointer_aba_retires_callbacks`。 | UI/native 布局和完整窗口行为未验收。 |
| ready（pointer 替换） | tab ID 不变，但新 `task_scope` 触发 SidePanel mount epoch 更新并撤销旧 token；`TaskPaneBinding` retire 旧 Run 与回调。加载 token 也阻止旧 pointer 回写。 | `ui/demo/tests/task_binding.rs`：`task_status_updates_keep_exact_composer_but_pointer_aba_retires_callbacks`、`changed_task_pointer_during_host_preparation_cannot_admit_the_old_click`；`ui/demo/tests/side_panel_component.rs`：`task_identity_is_stable_while_shared_resolution_retires_changed_or_missing_session_content`。 | 多 parent 窗口中的 UI/layout 实际呈现仍未验收。 |

ClientKit 对照：`/Users/frankqin/dev/TekesClientKit/Sources/Composer/Agent/TaskRunPanelProjectionState.swift` 同样以 task ID 做投影身份并拒绝过期 load；`ClientAgentTaskView.swift` 在未解析时显示 loading，解析无 task 后显示 `Task run unavailable`，并在 Run document ID 变化时替换嵌入 Run。当前 ClientKit 加载器以 optional 返回值表达结果，`try? await store.task(...)` 将读取错误折叠为 `nil`，所以 UI 最终显示 unavailable。AgentKit 保留 `TaskPanelState.error` 并显示原始错误文本。这是更细的诊断映射差异，不代表错误被当成有效 Task，也没有据此改写成 ClientKit 的丢错行为。当前 ClientKit checkout 下未找到这些 projection 行为对应的测试名，因此本审计只把它作为实现对照，不声称已有 ClientKit 测试验收。

## 子代理状态、读取与授权

| 场景 | 实现与可观察行为 | AgentKit 现有测试 | 仍未组合验收 |
| --- | --- | --- | --- |
| loading / 正在打开 | `begin_child_open` 生成带 revision 的请求；请求尚未成功前不安装子代理正文。SidePanel 的 parent projection 未就绪时撤销所有正文 token。 | `ui/demo/tests/side_panel_component.rs`：`awaiting_projection_retires_all_content_without_empty_or_stale_body_and_never_revives_old_tokens`；`ui/demo/tests/panel_navigation.rs`：`late_child_open_cannot_replace_a_newer_owner_of_the_same_tab`。 | 独立的可见“子代理 loading”状态、布局和完整窗口渲染没有作为本次目标验证；没有将加载态与全部父切换排列组合。 |
| missing / 父 pane 已撤销 | child 继承 direct parent 及祖先 authority；parent retired 时 child access 变为 Missing，不能再从旧 pane 获得控制能力。成功打开但 child 历史为空，与 missing parent 是不同状态。 | `ui/demo/tests/child_binding.rs`：`nested_child_inherits_retirement_of_its_original_parent_pane`、`task_to_another_scope_inherits_its_original_child_parent_authority`；`ui/demo/tests/panel_navigation.rs`：`closing_a_subagent_returns_to_the_last_task_without_retiring_its_binding`（该测试断言 child binding access 被撤销，父 Task 仍有效）。 | 子代理缺少统一显式 missing 空态；父撤销后的真实控件呈现没有组合验收。 |
| error / 打开或读取失败 | `ChildOpened(Err)` 仅在原请求仍 current 时发 notice，不安装 source；读取错误写入 `pane.read_error`，经 `ReadOnlyTranscriptInput` 传到只读正文。分页仍由用户明确重试，不自动刷新。 | `ui/demo/tests/child_binding.rs`：`builtin_bounded_history_pages_only_child_content_and_mounts_without_a_composer` 覆盖分页读失败后不自动重试、显式再次操作可继续；`ui/demo/tests/panel_navigation.rs`：`late_child_open_cannot_replace_a_newer_owner_of_the_same_tab` 覆盖旧请求不替换新 owner。 | 当前没有独立的 child-open error/missing 视觉状态断言，也没有一条测试同时检查错误正文、父 A→B→A 和关闭后的迟到错误。 |
| ready / 独立子 Run | `ChildPaneBinding::open` 固定 direct parent、child ID 与 layout；独立 session 可显示可读历史，但不因此获得主 Run 发送控件。 | `ui/demo/tests/child_binding.rs`：`nested_child_uses_direct_parent_and_existing_catalog_policy_without_changing_readable_history`、`late_child_open_caches_original_parent_without_stealing_selection_and_owner_drop_revokes_callbacks`。 | Native/Pi/DSH 三端的真实窗口和完整 SidePanel chrome 未验收。 |
| ready / 同 session 有界历史 | 只读 transcript 依赖 source、scope、publication、connection epoch 和 source-ready 检查；内建分页受显式 intent 与 epoch fencing 约束，不挂 Composer。 | `ui/demo/tests/child_binding.rs`：`builtin_bounded_history_pages_only_child_content_and_mounts_without_a_composer`；`ui/demo/tests/side_panel_component.rs`：`nested_child_parent_is_content_identity_and_foreign_parents_cannot_replace_it`。 | 这些是现有 headless/component 测试，不等同三端单独的完整 UI/native 验收。 |

## 父导航、关闭与迟到结论

| 组合 | 已核验行为 | AgentKit 现有测试 | 尚缺 |
| --- | --- | --- | --- |
| Task pointer A→B→A | generation token 和 Run ownership 都是单调生命周期；旧 A 的回调、prepared send、草稿权限不会因回到业务 pointer A 而复活。 | `ui/demo/tests/task_binding.rs`：`task_status_updates_keep_exact_composer_but_pointer_aba_retires_callbacks`、`changed_task_pointer_during_host_preparation_cannot_admit_the_old_click`。 | 没有把 SidePanel mount 的所有 ready/loading 状态与此 pointer ABA 放入同一测试。 |
| 父导航 A→B，A 的 Task 读取迟到 | 迟到结果可以缓存回原 parent，但不得抢走当前 B，也不应破坏 B 的焦点/草稿；显式选回 A 后恢复其 Task。 | `ui/demo/tests/panel_navigation.rs`：`original_task_completion_can_cache_after_parent_switch_without_navigating`。 | 尚无 A→B→A 加 Task pointer 再变更的全交错组合测试。 |
| 关闭 Task / 关闭 Subagent | Task 关闭选回最后一个 tab；子代理关闭回到前一任务 tab，并撤销 child pane 的访问。 | `ui/demo/tests/panel_navigation.rs`：`closing_a_task_selects_the_last_remaining_open_tab`、`closing_a_subagent_returns_to_the_last_task_without_retiring_its_binding`。 | 关闭与所有状态、父级重选、异步操作的笛卡尔组合未覆盖。 |
| 关闭或新 owner 后旧 child 结果迟到 | close tombstone 阻止尚未安装的较早 open 重新出现；更晚 owner 获准后，较早请求不能替换它。 | `ui/demo/tests/panel_navigation.rs`：`closing_a_child_fences_a_never_installed_older_open_result`、`late_child_open_cannot_replace_a_newer_owner_of_the_same_tab`。 | 未做实际窗口观察；跨 endpoint 及容器 transport 不在本次范围。 |
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

主会话完整 UI workspace 回归通过：634 passed、0 failed、1 项原有忽略（TekesUI #49）。其中 demo package 的 `task_binding` 10、`child_binding` 5、`side_panel_component` 11、`panel_navigation` 15 项均通过（41 项）；同名 consumer 测试文件另计，不混淆。完整日志 `.runtime-fixtures/parallel-issues-20261024/ui-tests-complete.log` 保留。严格 Clippy/格式通过。以上仍是组件/夹具运行；没有真实窗口、系统 IME 或全排列组合验收声明。
