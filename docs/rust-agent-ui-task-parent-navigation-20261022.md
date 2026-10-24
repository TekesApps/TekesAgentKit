# 任务导航与原请求完成的父会话边界（2026-10-22，实验状态）

对照只读 ClientKit `ClientAgentTaskView.swift` / `TaskRunPanelProjectionState.swift` 的原任务
load token、取消及 owner 生命周期，以及 Tekes `ClientAgentHostAdapters.swift` 的显式跨 thread
导航。本轮只修改 AgentKit demo 宿主，不修改共享 presentation、SDK 或这些外部仓库。

此前 B 为当前会话时，`open_task_source(A)` 仍可替换 A 的原任务 source、取消其旧 owner，
清除 B 焦点，并推进全局 panel 导航 revision，使 B 的 pending child 请求过期。
现在在任何这些变更之前，要求 source.parent 匹配 `current_binding()`；父会话已切换或清空时
返回错误。原 TaskLoadToken、runtime/source identity 和 pointer 检查保持不变。

宿主接入须区分两个动作：

1. **显式任务导航**：宿主先选择父会话，初始化原 TaskPaneBinding 的 task identity，调用
   `open_task_source` 保留这个 source；随后异步加载并用原 token commit。跨父会话导航由宿主
   明确执行，不能用晚到的 task 数据隐式解释当前选择。
2. **原请求完成**：原 source 得到合法 RunBinding 后调用 `install_task_run(original_parent,
   original_task_id, original_run)`。该入口仍允许在 A→B 之后缓存 A 的结果；不选择 A、不清除 B
   的焦点或修改 B 的导航代次。用户回到 A 时显示原 task run 和草稿，不回落父 Run。

本轮没有新增任务/草稿/执行状态机，也没有把数据安装等同于新导航请求。
`open_task_source` 是一次新的显式宿主动作，不能当作任意 await 的完成回调；
更广泛的异步导航队列/全部 A→B→A 组合仍需按原请求契约审计，不能只凭 parent 地址相等通过。

## 验证及限制

- UI workspace 508项通过，1项既有 #49 Popover 接收门禁明确 ignored；严格 all-target Clippy、
  fmt、构建、UI 边界和实际8包/241文件公开源清单通过，包含第二 public API consumer。
  共享组件 API 未改变，第二 consumer 不需要复制宿主导航策略。
- 新增3项回归：拒绝 inactive parent 替换且保留原 Task owner/草稿、B 焦点及 pending request；
  原任务完成可缓存到 A 且保留 B 文字/选区/编辑器 owner；CoreText 正式 Transcript 路径在 B
  中文 preedit 期间安装 A 的结果，完整 opaque baseline、marked text、原焦点与 canonical
  draft 保留，未提交。
- 初始测试把 focus 的变化返回值当作归属，并把 SDK 对新外部版本的 epoch 确认误判为文字
  变化。已改查实际 focused 与原 editor，并分别检查版本确认后的稳定性及文字/选区/owner。
  组字时的完整 baseline 断言未放宽。所有失败/编译诊断原样保留。
- 冻结程序 `.runtime-fixtures/AgentKitTaskParent20261022.app`，SHA256
  `5bf74a931e187dc9906277d8699a027d0f1c78d2f8fe5eff7873e138f26123b1`。
  原生组合探针在切到 B 后失去激活，前台轨迹切回 Codex，停在 phase13（要求实际 B 编辑器
  焦点和激活）。新增的 inactive-task 原生断言尚未执行，无完整成功收据；**本轮原生组合未通过**。
  没有强制激活、移除焦点条件、改成更短成功模式或覆盖失败。

真实 Native0.9.3/Pi1.3.0/DSH0.2.1-rc.2 的同程序数据到离屏 GPU smoke 单独记录，不能证明
新增任务导航、原生 IME 或真实三端 task pane 场景。Native 使用明确 UI-only 范围，
完整 lifecycle/archive 仍不在该通过范围。系统拼音本轮未重跑，其前次一次完整通过和随后
失焦拒绝仍保留原程序归属，稳定性尚未完成。

精确 TekesUI 公开 pin 仍为 `09b4bab44b6dfa65487250ea816b610b9df59095`，ProcessContainer
#48 仍 OPEN；没有发现能独立归因 SDK 的新组件缺口。全部 Views 对齐、跨进程、实体键盘/
候选窗/VoiceOver 与最终验收仍未完成。PR #10 保持 draft，本阶段不合并、不打 tag。

原始日志、失败轨迹、程序/源码哈希及三端结果在 `.runtime-fixtures/task-parent-navigation-20261022/`，
摘要见[机器记录](rust-agent-ui-task-parent-navigation-20261022.json)。复用现有 target，未新建 target，
未删除缓存，释放空间0；保留源码、固定依赖、失败记录和验收程序。
