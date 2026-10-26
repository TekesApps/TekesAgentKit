# 真实 Pi 目标回传与窄栏脚注失败（2026-10-22）

本轮复用 ae5f015 阶段的最终程序，没有修改或重编 Rust UI、presentation、绑定或 TekesUI。
程序字节、Rust/Cargo 输入与原保留 manifest 核对一致；新增的只有验收工具、测试和证据。
公开 TekesUI 仍固定 `50ec49f3446924fdbc66b3cff22cd8cfe150fe7c`，Pi 运行时为 1.3.0。

## 已验证的数据链

新增工具选项 `--ui-goal-footnote`：要求 Pi + components + Transcript，与 native/history/bounded
专项互斥。使用独立临时 session 和本地受控 provider，先取得真实最终回答，再经现有公开
`goals.create` 创建 disarmed 目标，捕获原 ref 后暂停。原 UI consumer 通过外部 endpoint 读取
实际 projection，原 footer 的 scope、goal ID、revision 2、paused phase 与公开 get 结果一致。
没有 `--goal-footnote-fixture`、私有会话文件读取、合成 goal DTO 或自造 RPC。

随后用该已捕获 ref 做 CAS clear，公开 reconnect/get 返回 null；新 UI consumer 的
`goalFootnoteProof` 明确为 null，原对话 message、turn、footer 行数量保留，没有多出诊断行。
原 created/paused/clear ref 的 ID 一致、revision 递增，完整公开方法结果、两次 consumer stdout/stderr
和图片保留。这是两个新 consumer 的离屏数据/生命周期证据，不是原已挂载视图的 SSE 更新、
真实键鼠、中文 IME、VoiceOver 或 ProcessContainer 证据。

Native/DSH 没有此验收路径使用的标准化 `goals.create` 别名，因此本轮目标专项未覆盖它们；
这不等同于认定不存在其他公开创建方式，也不替代三端正常回归或完整目标验收。

## 完整 UI 门禁仍失败

在冻结程序默认的 1068×760 宿主、约 320pt 会话列中，原按钮的文字 fallback、反馈状态/刷新
占满单行 aside；目标标签虽然已在原节点中正确生成，仍被裁出可见区域：`labelVisible=false`。
工具保留 `footerVisibilityAccepted=false` 并以退出码 1 结束；数据链通过不提升为完整 UI 通过。
最初未跑到 clear 的失败也保留，后续补充收集独立 clear 证据仍保留同一严格可见性断言。

对照当前 ClientKit/TekesMetalTextKit 的 `appendAside`：output lane 按序左对齐，action 使用紧凑
controlSlot，普通 information 使用剩余宽度。它不是靠新增多行布局来遮掩溢出。
公开 TekesUI 的 `ButtonTextRenderer` 仍无可取得原 IconRenderer 的服务入口，Content 原图标/原紧凑
按钮 glyph 组合缺口继续关联 [TekesUI #47](https://github.com/TekesApps/TekesUI/issues/47)。
AgentKit 不复制图标清单、atlas、事件/绘制引擎，也不把按钮文字裁成假图标、隐藏动作或改成多行来过关。

图标能力不是唯一待办：消费侧反馈 cold/loading/failed 文案与刷新入口仍需继续核对 ClientKit，
不能把所有额外宽度都归为 SDK 缺陷。本轮没有为了让目标标签可见而删除这些状态。

## 工具、复用与交付状态

原始运行示例（使用现有已建程序和对应正式 Pi runtime）：

```text
scripts/test-real-session-endpoints.py --pi-root <Pi 1.3.0 runtime> \
  --ui-demo <frozen AgentKit demo executable> --ui-output <new-prefix.png> \
  --ui-components --ui-transcript-content --ui-panel-navigation-toolbar --ui-goal-footnote
```

当前预期会在可见性门禁失败，不能把 `dataLifecycleAccepted=true` 当作命令成功。
普通验收模式默认不执行此新专项。新 pure verifier 分别检查数据身份与完整可见性；
57 项工具测试通过，负例覆盖 scope/revision/phase、伪造来源、旧 consumer 缺少 proof、stale history、
clear 后诊断/对话变化及 CLI 在启动前拒绝不兼容组合。

[机器收据](rust-agent-ui-real-goal-footnote-20261022.json)与原始证据目录
`.runtime-fixtures/real-goal-footnote-20261022/` 保留部分通过与明确失败；约 0.8 MiB，未新增程序副本或 target，
未删除文件，释放 0。PR #10 仍为实验草稿；当前目标与完整 UI/原生/跨进程验收仍未完成。
