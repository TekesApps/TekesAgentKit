# 原反馈首次读取与重连接入

继续消费精确公开 TekesUI `50ec49f3446924fdbc66b3cff22cd8cfe150fe7c`。
对照 ClientKit 会话打开/历史恢复后的 `synchronizeMessageFeedbacks`、重连 touched-session
刷新及 `feedbackPresentationTargets`，补齐现有 Rust 宿主的读取调度，不新增反馈存储或写入策略。
共享包仍以 v0.3.8 为已发布基线；这里是显式仓库内开发路径，尚未发布新契约。

## 行为与宿主接法

`RunBinding.feedback_read_epoch()` 提供可读连接代次；`read_feedback_at_epoch(epoch)`
在读快照和原 `call_at_epoch` 中核对捕获代次，不跟随新连接、不重试请求。
已有 `read_feedback()` 和原 FeedbackFrame 的 source/publication/epoch/CAS 校验保持。

ComponentDemo 对每个已挂载来源/连接代次排入一个 `DemoCommand::ReadInitialFeedback`。
宿主在现有 `take_background_commands()` 通道取得请求，异步执行 `request.execute()`，
再调用 `complete_initial_feedback(completion)`；原生 demo 的原命令/完成通道已接通。
只读、不支持、断连和已撤销来源不调度。失败不影响正文或草稿，也不在 draw/sync 中循环重试；
仍通过原反馈 Refresh 手势明确恢复。初次成功的空列表是已加载状态，不是缺失记录的猜测。

请求捕获原 binding、pane、连接代次及挂载生命周期。手动刷新/写手势使初次请求失效；
未执行的旧请求不发 IO。跨重连、移除/重新挂载或较晚 publication 的完成不采纳；
隐藏 A 的合法完成只更新原 A 的元数据，不写入当前 B。结果仍由原控件和 FeedbackFrame 校验。

compact 输出 aside 不再添加 cold/loading 的常驻提示文字；pending 禁用、真实错误及不确定恢复提示保留。
`TurnFeedback.initial_read_failed` 不产生空 committed record、不授予权限，也不替换编辑器或手势。
普通独立编辑器的状态提示不变。

## 验证与未完成范围

7 项新增宿主/受控 endpoint 回归覆盖首次已提交备注、空列表、重复 sync、失败与明确恢复、
排队旧代次无 IO、跨重连迟到完成、隐藏会话隔离和宿主销毁。原 feedback_read 最终共 13 项通过。
完整 UI workspace 542 项通过、1 项既有 #49 忽略；之后新增两项隔离测试单独通过，
最终独立 consumer 22 项通过、1 项相同忽略，workspace/all-targets 严格 Clippy 通过。
独立 consumer 验证 cold caption 不常驻、失败不会制造记录、已提交 frame 不受迟到读取错误覆盖，
并沿用原 editor/中文 preedit/复制及布局回归。

这是同进程组件、宿主及公开协议受控 fixture 证据，没有打开窗口或操作桌面。
本次代码的 Native/Pi/DSH 真实运行、原生 IME、GPU 首次读取和跨进程尚未验证。
离屏静态 snapshot 模式不驱动原生命令循环；不能把它算作自动读取验收。
此前冻结的原生程序未改写，历史原生/真实 harness 结果不转移至本次代码。

常驻 Refresh 入口仍保留，完整 compact aside 恢复入口及 glyph 对齐仍待完成；
TekesUI #47 原图标/紧凑动作消费接口与 #48 ProcessContainer 仍开放。
真实 Pi 的旧版 footer 可见性失败没有因此关闭，必须在完整布局修正后重新运行严格门禁。
本增量不修改 TekesUI、不隐藏业务动作、不增加换行或自绘图标以绕过验收。

[机器记录](rust-agent-ui-feedback-intake-20261022.json) 保存源码和日志哈希。
原始失败/中间日志一并保留；首次回归发现的额外排队请求已用生命周期取消修正，编译缺字段错误已修复。
复用 `/tmp/agentkit-layout-text-292-target`，无新 target、无程序副本；没有删除缓存或证据，释放 0。
PR #10 保持 draft，不合并或打 tag。
