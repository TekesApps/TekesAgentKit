# TekesUI #27：292 独立候选消费

本轮只消费通用文本/布局契约，不修改 TekesUI、Kernel 或 Swift 实现。默认公开 pin 仍为 `8cf0840790334ba224afc0595897c74231c712e6`，原 545 与其它冻结包保持独立。

## 精确输入与原始探针

归档 `layout-text-292f5fe65195.tar.gz`，SHA-256 `e9a816c5e34681f92d6632e1abe5995329f52a6d0757cf243df75aaf554fea76`；源码身份 `292f5fe65195af66624cb28413868eb46db6ab1a81ef1294ab19c8efbd45b0f4`。接收方核对 479 文件与相对原 545 的 8 路径增量：通用 composition、一个语义 JSON 及生成物、文档和最小 consumer。没有混入后来的集合、Tooltip 或其它 WIP。

原始 `tekesui-active-layout-preflight.rs` 保持原样。它在旧版本断言已知缺口：layout 内从 inactive 变为 active 时，错误 renderer 会被错误地允许继续。针对 292 的原始重放在该断言上失败，返回 `MissingTextServices(ComponentId(11))`。这是预期的负对照，不是把失败测试计成整包通过。

## 正式 AgentKit 消费

`--append-run-probe` 将新的 receiver 片段追加到**未改动的正式 RunView 事务探针**，复用它的实际 RunView、Composer、Transcript、CoreTextShaper 和库 reading attachment。没有替代视图、第二套编辑器/状态机、消费侧排版补丁或像素滚动补偿。

新增测试覆盖 3 档字号、2 个宽度、宽/高为零两种折叠方式，共 12 次完整失败/恢复路径；另补 6 次失败期间跨宽度恢复：

- 始终 active 的 Run 与重新展开的 Run 都要求文本服务；入口已 active 的折叠帧仍需合法 backend，隐藏后才可移除。
- 失败发生在绘制之前；Scene 为空，不调用文字 draw。应用侧 arrange 激活变化保留，不能把 `is_pane_presented` 当成功帧标志。
- 原编辑器地址、正文、external baseline、Transcript 行身份、reading document version 保持；合法 renderer 重试后原阅读 item 与 y 锚点恢复，不自动 follow latest，不抢焦点。

原 8 项 RunView 事务测试一起通过，包括 IME/编辑 ABA、延迟发送清空、旧 reading token、同 publication 的关闭重开、追加/前插及 Composer 高度变化。另以正常 renderer 重跑候选下的整个 AgentKit UI workspace；其中包含独立公开 consumer。精确数量、输入 hash、日志与各证据范围见 [同名 JSON](tekesui-layout-text-candidate-20261021.json)。

重放入口：

```sh
PYTHONDONTWRITEBYTECODE=1 python3 scripts/test-reading-candidate.py \
  --source <292-source> --manifest <portable-manifest.json> \
  --probe scripts/probes/agentkit-layout-text-candidate.rs --append-run-probe \
  --workdir <new-receiver-directory> --target-dir <owned-cache>
```

需要 macOS/CoreText、Rust 1.95.0 和 Python 3.11+。候选路径覆盖仅存在于新副本；不改默认清单/lock、冻结包或候选源码。

## 成功帧与原生桥责任

业务 publication/epoch 校验与渲染成功是两道独立要求。只有 `ContentHost::render` 返回 `Ok` 后，宿主才可发布该帧的原生文本状态、AX、可点击几何与回合可见性。`RunView::layout_geometry` / `turn_targets` 是已 arrange 值，不是 successful-render receipt。失败帧可能保留 app layout 变化与旧 paragraph 缓存；两者都不能作为当前原生排版的权威。

错误 renderer 的诊断/恢复宿主应清除或隔离失败帧的原生发布和新输入，仍允许既有 native text-key 事务的 Finished acknowledgement 收尾。库负责文本依赖扫描、输入取消与焦点；宿主负责窗口的发布边界。保留原组件，恢复合法 backend 后重试；不要为解决渲染错误重连 harness、重发 prompt、换编辑器或覆盖草稿。

当前 AgentKit demo 正常路径始终提供实际文本 backend；渲染错误 fail closed。通用 SDK 的诊断模式与生产 RunView 验收须分开记录，不能将其 generic Pane 当 AgentKit 标准组件，也不能用生产者另一源码身份的 native/GPU 收据证明本候选。

## 结论范围

这里的组件/CLI 结果只证明精确输入的可消费性和恢复契约。原生窗口、GPU、物理 IME、VoiceOver、ClientKit/Gold7 产品页对照分别记录；未执行的项目不由构建或发送方报告替代。[TekesUI #27](https://github.com/TekesApps/TekesUI/issues/27) 与默认公开版本的缺口仍保留，不能自动关闭、提升冻结包或替换公开 pin。完整 Agent UI 的其它布局和搜索成员关系工作仍开放。


接收方另以原封不动的 SDK consumer `layout` 模式在 U2790B@2 运行，记录 14 个活动检查点：两次布局激活失败、一次已聚焦字段的 backend 丢失、失败帧的新键输入/Tab 隔离、合法恢复不抢焦点、三次真实 Tab 入焦与字号往返。原 baseline/value 保持；失败时清空原生字段节点和 text input 发布。窗口画面已现场检查，收据保存完整状态检查点，不声称这些截图已有独立 PNG 归档。VoiceOver 未开启，未测试系统中文 IME。SDK 窗口与实际 AgentKit 窗口分开顺序运行。关闭后的 AX 观察意外启动了一次默认 list 演示；该额外进程已关闭，不计入 14 个 layout 检查点。

实际 AgentKit 正常 renderer 的原生总回归及使用 292 的 Summary 标签/折叠窗口另有记录。SDK text 的 432 组/864 帧 label GPU 回归验证原 glyph 结果未回退；它不是 #27 专项失败/恢复 GPU 对照，也不替代产品页视觉验收。

接收结论：此精确候选的接入契约和上述 receiver 回归通过，建议作为下一轮受控开发冻结的输入；本轮未执行冻结提升。公开 pin 仍需对应不可变发布版本与单独升级检查，不能因此把默认 8cf 的 #27 缺口标记已修复。
