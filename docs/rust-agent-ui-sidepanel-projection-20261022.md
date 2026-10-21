# SidePanel 父投影、生命周期与最终原生回归

2026-10-22 重新核对 TekesUI 公开 main 和 annotated tag `remediation-toolbar-checkpoint-20261021`：均为当前精确 pin `09b4bab44b6dfa65487250ea816b610b9df59095`，无需重复升级。#22/#28 仍 OPEN，但所需 reading/reveal API 已公开并被正式 demo 消费；issue 状态不代表 API 尚缺。没有修改 TekesUI 或使用活动工作树覆盖。

## 与现有 Views 对照

只读对照 ClientKit `Sources/ToolPanels/ClientSidePanelHostView.swift::reconcile/evictContent`：同 thread 的 tab 保留实际内容实例；thread 改变时清空旧实例；已选 thread 的投影未到时不构造正文。Tekes `Tekes/SidePanelHostView.swift` 仍负责宿主 toolbar placement 与窗口 contentTopInset。本次补前者生命周期，不将 toolbar、card/inset 或完整配对视觉视为完成。

`SidePanelInput` 新增必填 `projection_ready`，独立于连接状态和子历史加载。已知父投影的 `SideChatBinding::input` 默认 true；外部宿主明确覆盖，第二公开 consumer 暴露 `sync_scratch_tabs_with_projection`。等待时清空正文与 retained UI、返回 None content token、禁用创建及 SideChat close，保留导航元数据；不显示伪空态。跨 endpoint/thread 销毁 UI，同 thread 的 tab/scope 缓存仍有界。

等待往返、thread 切换使旧 token 失效；相同 revision 的冲突输入拒绝。业务草稿、运行及 SideChat pointer 继续归原 binding/controller，UI 销毁不取消任务或 discard。宿主先完成 pane 组字并释放其 SDK focus，主编辑器组字不受影响。子请求捕获父投影 generation，晚回包可保留有效 binding/pointer，但不能接管新一代选择。

修复宿主在旧 pane 销毁后仍持有 document_roots 的问题：重新挂载使用新 Transcript namespace/SDK reading authority，草稿从原 presentation 恢复。销毁后持久阅读 checkpoint 仍跟踪 TekesUI #42，不能把新建 authority 当成已完成阅读位置持久恢复。

## 验证与证据范围

UI workspace 444项测试通过，包含第二公开 consumer、同 thread 保留/跨 thread Drop、等待空正文、旧 token、冲突 revision、晚回包及草稿零提交。最终 all-target strict Clippy、fmt、原生构建和 Rust 边界检查通过。

最终程序 SHA256：`2e957e333db684f94a13a5fee92b2f015ff85618c58b49dabe354d63bab5c10e`，保留在 `.runtime-fixtures/AgentKitPanelProjectionFinal.app`。以下证据全部来自同一个最终程序：

- `--panel-projection-selftest`：原按钮创建 SideChat，等待时正文为空；ready 与 A/B/A 重新挂载产生新文档/token，草稿及 pointer 保留，主编辑器原 focus/marked baseline 不变，零提交。
- `--native-selftest`：实际 Sidebar 点击、发送/流式/取消、草稿隔离、重连及 SideChat/task/subagent/只读子历史路径通过；此模式不代表正式混合 Transcript 阅读验收。
- `--reading-native-selftest`：实际分页按钮20→44项，前插局部锚点、向上阅读、A/B独立阅读与草稿、重连、Start/Latest/Jump、原编辑器/focus/中文预编辑 baseline 均通过，无提交。这是 owned SEP fixture，非本轮真实三 harness 验收。
- 系统简体拼音：对独立 PID 发键，`ni hao` 保持组字，发送/导航屏障有效，空格提交“你好”后单次发送；输入源前后相同，未改用户输入法。软件输入法证据，不代表实体键盘、候选窗或 VoiceOver。

`.runtime-fixtures/sidepanel-projection-20261022/` 保留源码哈希、程序哈希、测试/Clippy/构建日志及原生收据。首个程序与日志也保留：其 projection 专项通过，但综合脚本在 pin 重排前捕获 Sidebar 行坐标，随后误点；修正为等待 pin 事实发布并 render 后采集实际坐标，再运行原始行点击与断言。未改业务断言或强制激活窗口。`native.png` 属于首个程序的专项截图，已人工检查用户消息/footnote居右、机器内容居左及独立 pane 草稿；不作为最终程序截图或完整布局验收。

仍只消费 TekesUI。当前未发现可独立归因的新基础缺口，不重复提 issue；原 #42（重建后的持久阅读）、#44（原组件上下文菜单）、#36（toolbar）继续跟踪。完整 UI/配对布局仍未完成，PR #10 保持实验草稿，不合并或打稳定 tag。

复用 `/tmp/agentkit-layout-text-292-target`，debug0、incremental关闭，没有新建 target。本阶段未删除缓存，释放空间为0；源码/WIP、固定依赖、失败记录、验收程序及证据保留。
