# Feedback 初次加载与输出 aside 对齐审计

当前 AgentKit `5652f54` 与本地 ClientKit 源码对照，发现消费侧待修项；不是 TekesUI 基础能力缺口。
源码哈希保存在 `.runtime-fixtures/native-resume-20261022/feedback-contract-source-audit.json`。

ClientKit 的 `SessionEndpointRuntime` 在打开/恢复会话后调用 `synchronizeMessageFeedbacks`（976、1151 行），
以原 session 队列串行加载并发布 loading/ready/failed；重连对 touched sessions 重同步。
`ThreadTranscriptPresenter.feedbackPresentationTargets`（1542 行）只将加载失败或单条操作失败附到目标。
普通 cold/loading 并不成为输出 aside 的附加文字。

AgentKit `ui/demo/src/component_feedback.rs` 只在手势请求后取得 frame；`sync_feedback` 对没有 frame
的控件 invalidate，但没有初次加载调度。`ui/components/src/feedback.rs` 的 render 在 compact aside 中
也常驻 Refresh，cold 时附加 unloaded，pending 时附加 pending。这与上述显示/加载行为有差异，
会增加真实 Pi 目标 footer 已观察到的宽度压力。现有 CAS、epoch/publication、原 binding 与原控件
保持机制仍须保留；不能只隐藏刷新入口而让 cold 控件永久不可用。

## 需要一并完成的修正与门禁

1. 沿用原 RunBinding.read_feedback，提供可供宿主调度的初次/重连读取及完成采纳路径；
   一个 source/连接代次的初次读取不因 draw/sync 重复发起，失败后由明确恢复动作重试。
   切会话、关闭、断连或更晚 publication 后，迟到完成不能更新另一控件或恢复旧写权限。
2. compact aside 按 ClientKit 仅显示真实失败/不确定恢复信息；cold/loading 不追加常驻状态文案。
   保留 pending 禁用和实际错误恢复入口，单独反馈编辑器的状态提示与中文组字不受影响。
3. 验证首次加载已提交的 rating/note，空列表为明确 ready；断连期间不可写，重连恢复后不重放 mutation；
   read/write 并发和 out-of-order 完成仍遵循原 publication/CAS；未读不能视为无评价。
4. 组件、真实宿主、第二公开 consumer，以及真实 Pi goal/footer 严格可见性门禁均须重验。
   这只修正业务显示和读取接入；TekesUI #47 原 glyph/compact action 服务仍需公开交付，
   不用隐藏动作、换行或自绘来保证窄列通过。

本记录是源码审计与待实现门禁，不是已完成修复。当前真实目标数据闭环通过、footer 可见性失败的结论不变。
`5652f54` 的四项 CI 已通过；它们不覆盖上述缺失的初次加载行为或当前原生系统提示阻挡。
