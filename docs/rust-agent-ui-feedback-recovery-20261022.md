# 反馈恢复入口与三端离屏复验

在 `4d3096c` 的首次读取/重连保护之上，继续消费精确公开 TekesUI
`50ec49f3446924fdbc66b3cff22cd8cfe150fe7c`，AgentKit UI 仍为显式仓库内开发路径。

## 对齐与行为

对照 ClientKit `TranscriptTurnFootnote.outputNodes` 的动作/备注/失败/时间/目标顺序，
及 `feedbackPresentationTargets` 的失败显示规则；Tekes 的 `ClientAgentHostAdapters` 仍将内容委托给 ClientKit。
正常 compact aside 的 cold、loading、ready 均不挂载常驻 Refresh。初次加载失败、未知结果或
已提交 frame 失去 authority 时保留明确恢复入口；初次读取失败使用可由宿主替换的
`FeedbackLabels.load_failure`，默认“无法加载评价”，不与写入失败混用。
隐藏刷新入口不能经原动作接口派发；原 target/publication/CAS/epoch 和组字禁用保持。
异常恢复入口的显示与完整图标样式仍按实验范围记录，未宣称完整 ClientKit 视觉一致。

离屏 demo 通过 `take_initial_feedback_requests()` 取得原排队请求并调用同一个完成采纳接口，
不会丢弃其他 background commands。原生事件循环仍消费原通用命令通道。
原生反馈探针改为等待自动初读；删除后点击原“重新连接”按钮，要求新 epoch 和更晚 publication
再确认无评价，继续检查原草稿、焦点及内容。这个更新只完成编译，没有在本轮打开窗口执行。

## 验证结果

- UI workspace **544 通过、1 项既有 #49 忽略**；包含原组件/宿主及第二公开 consumer。
- 严格 workspace/all-targets Clippy、demo build、格式和静态边界通过。
- 同一冻结程序下，Pi 1.3.0、DSH 0.2.1-rc.2、Native 0.9.3 的真实公开协议→原 RunPort/
  Transcript 反馈自动初读、中文备注保存/重开、同评价删除、删除后新读、草稿保持和 GPU 输出通过，driver 退出 0。
  使用隔离的自有会话和本地受控 provider；Native 明确 `--kernel-ui-only`，不包括 lifecycle/child。
- 本轮真实目标复验严格退出 **1**：公开 Pi create/pause/get 的目标数据与原 footer 一致，
  clear/reconnect 新 consumer 不再保留旧目标，但窄列 `labelVisible=false`，未通过 UI 可见性门禁。

独立布局测试继续核对单行、完整 source/AX、动作顺序、备注 40% 上限、剩余信息宽度和原 editor/marked baseline。
移除常驻控件后，宽列可容纳更多文字，故截断断言固定在明确的 180pt 窄列；没有把“必须截断”当作宽列的显示要求。
旧测试中手动 cold Refresh 改为原自动初读；迟到目标完成测试先建立实际失败恢复状态，再验证原拒绝门禁。
所有早期失败输出均保留。

反馈 proof 的删除后外部新读会使旧 UI frame 失去 authority；它的最终静态图不能外推为始终 ready 的普通运行布局。
正常 ready 隐藏 Refresh 有独立原控件/布局断言，完整反馈/GPU收据按各自范围记录。
本轮没有原生窗口、系统 IME、实体键盘、VoiceOver、跨进程或原生重连验收；历史其他程序的结果不转移。

## 仍未完成

正文动作仍使用文字 fallback；目标专项图中，复制、评价、分叉占满约 320pt 的列，目标/时间仍越出 clip。
TekesUI #47 需要正式的原 Content 图标/紧凑动作服务；不以隐藏动作、改多行或自绘图标绕过。
#48 ProcessContainer 等既有缺口仍开放；本轮只提供同进程宿主内容路径，不是窗口级 toolbar/跨进程交付。
PR #10 继续 draft，不合并、不打 tag。

## 证据与资源

[机器记录](rust-agent-ui-feedback-recovery-20261022.json) 保存各端完整回传、目标失败和证据哈希。
同一程序、源码输入及图像在 `.runtime-fixtures/feedback-recovery-20261022/`；
原工作区测试/Clippy/build 输出在 `.runtime-fixtures/native-resume-20261022/feedback-recovery-*.log`。
复用现有 UI target 与 root target，无新 target；保留一个约 69 MiB 的验收程序及图像/日志。
程序输入清单限定为仓库 Rust/Cargo/mjs 核验输入，不声称完整编译器/外部依赖环境闭包。
没有清理未知共享缓存或历史程序，删除字节与释放空间均为 0。
