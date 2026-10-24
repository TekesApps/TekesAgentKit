# 已提交反馈的重连加载失败

对照 ClientKit `loadMessageFeedbacks` 的失败发布及 presenter 的 `feedbackPresentationTargets`：
读取失败应显示已知加载错误，同时保留上一次已提交内容。Tekes 宿主仍委托 ClientKit 的 Run 内容。
继续使用精确公开 TekesUI `50ec49f3446924fdbc66b3cff22cd8cfe150fe7c`，不修改 SDK 或 Swift 参考源码。

新增宿主回归先观察到：重连后的无效公开 list 回包被拒绝，旧 frame 已失去写权限，
但 `initial_read_failed` 因存在旧记录而直接忽略，留下泛化的未知结果提示。回归修改前退出 101。
修正允许已撤销 authority 的旧显示接受已知读取错误；当前有效记录及 pending 手势仍拒绝迟到错误。
不清除旧 rating/note，不授予权限，不替换编辑器，也不自动重试读写。

修正后验证：已有正向评价/中文备注→重连→无效 list→保留原记录和加载失败提示；
positive/negative/note 原 SDK 按钮均禁用，三次 sync 无额外读取；明确 Refresh 返回可信空列表后，
较新的 epoch/publication 才清除旧记录、恢复原控件状态并隐藏恢复入口，草稿保持。
服务仅实现 list，任何误发的 put/delete 都会使测试失败。
独立 consumer 另外验证旧显示撤权后的错误展示、新 publication 恢复，以及有效记录不受迟到错误覆盖。

本轮 feedback_read **14**、feedback_write **22**、第二 consumer **22** 通过，1 项既有 #49 忽略；
workspace/all-targets 严格 Clippy、格式与边界检查通过。属于同进程受控公开回包的组件/宿主证据。
没有本轮完整 workspace 重跑、真实 harness 故障、原生窗口、IME 或跨进程验收。
此前 `4097e4f` 的完整 544 项及真实三端 GPU 证据保持原源码归属，不转移为本轮验收。
目标窄列可见性 #47、ProcessContainer #48 和当前程序原生验收仍未完成；PR #10 保持 draft。

[机器记录](rust-agent-ui-feedback-reconnect-failure-20261022.json) 保存源码及日志哈希，原始回归失败与通过日志均保留在
`.runtime-fixtures/feedback-reconnect-failure-20261022/`。复用原共享 target，没有新增 target/程序副本；
无缓存删除，释放空间 0。
