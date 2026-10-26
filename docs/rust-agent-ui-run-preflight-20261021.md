# 正式 RunView 的整帧准备与候选阅读事务

这是 AgentKit 正式 RunView 的公开 API 和独立候选集成，不是默认原生宿主已经完成阅读迁移或最终验收。默认固定公开8cf输入不变，UI继续0.0.0 / publish=false。

## Agent 与基础库的责任

`RunView::prepare_update` 接收同一原 binding 的 RunSnapshot、ComposerAffordances、ComposerControlGate、TranscriptPagingFacts 和 actionable controls，返回借用 snapshot 的 `RunViewUpdate`。它只做 Agent 业务预检：revision/epoch/scope、原 Composer 目的地、context 卡片容量/版本、分页字段、actionable、Indicators 版本及规范化 Transcript 和行 namespace。不修改控件、分配行 ID、收集本地 edit 或执行 RPC。

计划提供有序 item_ids，以及可选 `ComposerDraftUpdate`：原 TextEditBaseline、DraftRevision、selection 和 undo 参数。现有同步路径与准备路径复用同一 Agent 策略，没有复制通用编辑器校验。None 表示权威正文未变且有本地未确认输入/preedit，必须保留原编辑器，不伪造一次写回。

`RunSnapshot` 现从公开 components 包重导出，第二公开 consumer 的 `prepare_original_run` 消费捕获的 PaneSnapshot；Missing 和 ReadOnlyChild 明确拒绝创建可写事务，不重采样当前 binding 来替换原 frame。

## 同步提交顺序

先通过既有 update 完成原 Run 的初始 Composer 挂载，再为原 Transcript viewport 附加库的 reading document。准备结果不是 endpoint 权限、跨事件 reservation 或应用回滚承诺。

仅在明确固定的新 TekesUI 开发候选中执行以下顺序：

1. 根据原 paired frame 准备 Agent 计划；异步结果安装仍需原 mount/source/read token，不能把新 selection/epoch 冒充原请求。端点读取的归属/cut 校验仍由 binding/runtime 负责；UI对已验证的 paired canonical frame 同步准备，不能拿迟到的独立结果直接写树。
2. 事务入口调用 `validate_prepared_update`，重检 publication generation、gate、原 editor baseline、draft 参数和行身份。即使 canonical revision 没变，权限刷新 close→open、独立分页/等待控件刷新及 feedback 配置也淘汰旧计划。
3. 对 controlled_draft（若有）在原 TekesUI surface 调用 `check_external_value`，使用计划给出的全部参数。Conflict 时不进入 reading callback。
4. 以原 ReadingDocumentVersion 调用库 update_scroll_reading_document。旧 token 由库在回调前拒绝。
5. 同 UI turn、无事件/yield/任意应用 mutation 交错，在回调内 apply_prepared_update；组件再次检查计划并通过原库 apply 更新原编辑器及整帧。不能依赖库回滚任意应用失败。

`apply_prepared_update` 的接口在默认8cf也可编译；但仅调用它并不能补上旧库缺失的受控只读预检。默认宿主未自动改走新事务，不把 API 存在当作闭环交付。临时计划借用 immutable snapshot，不新增 Run 状态机或历史镜像。

## 独立候选与测试

测试源为 [`scripts/probes/agentkit-run-preflight-candidate.rs`](../scripts/probes/agentkit-run-preflight-candidate.rs)，直接挂载正式 RunView、Composer、Transcript。用真实 CoreText 测量，绘制回调为空，属于公开 API/几何/状态证据；不是 AgentKit GPU 像素、原生输入或真实 SEP 分页验收。

候选使用已独立回放的463文件 `2b16eaa6200fecdde9a0c615166064effb9446249cde05087d5d7c288d81d4ec`。现有 runner 支持显式 --probe，只改隔离 AgentKit 快照的依赖和已记录的AX required适配：

```sh
python3 scripts/test-reading-candidate.py \
  --probe scripts/probes/agentkit-run-preflight-candidate.rs \
  --source /absolute/verified-2b16-source \
  --manifest /absolute/portable-manifest.json \
  --workdir /absolute/new-owned-agentkit-run-replay \
  --target-dir /absolute/owned-receiver-cache
```

公开检查分别观察原 editor 身份/基线/值、选区/preedit/焦点、Transcript 行和 reading version；同一 scope 的两个窗口也不能交换 editor baseline。整帧的前插数据必须同时提供 turn 与 item；仅插入孤立 item 会正确进入未归属段，不能当作已前插历史。

最终七项候选检查：

- 合法整帧更新同时安装草稿与 Transcript，并用旧 baseline 的精确 ack 证明原 editor 实例保留；旧 reading token 不安装下一帧。
- Composition 冲突、编辑→撤销 ABA 在进入阅读回调前拒绝，基线、消息树和 reading version 不变。
- 权威正文未变时保留本地 preedit、焦点及选区，同时追加 Transcript，不生成 Send。
- Revision 冲突及同 scope 第二窗口的 ForeignEditor，分别验证库结果和 Agent 原计划守卫，不修改任一原窗口。
- 实际 RunView 中，Composer 高度变化后的追加和提供 turn+item 的前插保持同一阅读项及其 y 位置（不超过1点误差），只通过公开 wheel 输入改变阅读状态，不计算像素补偿。
- 一次 Send gesture 只产生一个原目标 intent；迟到清空携带旧 receipt 时保留发送后的新输入，不安装对应 Transcript 帧。该项没有派发 endpoint RPC。
- 同 publication 的 close→open ABA，以及独立 paging/actionable 刷新，在 controlled editor 本身仍可接受时由 Agent generation 提前拒绝，reading version 不推进。

完整通过项、最终测试数、失败探针与日志哈希见[JSON 收据](rust-agent-ui-run-preflight-20261021.json)。被保留的失败包括误写测试 target、第二 consumer 漏处理 Missing 枚举分支，以及初始前插 fixture 缺少 turn；未弱化原断言或改动 TekesUI 实现来绕过。

## 剩余验收

正式默认宿主还未消费这套 reading transaction；候选需要完整原生窗口、Native/Pi/DSH 真能力下的追加/恢复/分页、物理 IME/候选窗与最终并排布局验证。共享会话列API已实现，但其320宽度下限、rail净空与比例归一化尚未严格匹配ClientKit，不能视为列布局已验收；完整 Composer/chrome 叠层、回合 rail、#25 富内容与持久 checkpoint 仍未完成。#26 尚无公开 Git pin，Issue仍OPEN，未提升冻结、合并或打 tag。Layout/显示规则必须严格对齐当前 ClientKit Views，基础视觉继续使用 TekesUI默认值/tokens/主题。

最终默认 UI workspace 296项、Python脚本26项、两侧严格Clippy、fmt、边界与固定八包199输入检查通过。默认同一二进制的正式RunView/Transcript context专项及整体组件native fixture回归通过，已查看context截图；原日志中的transcript/reading-anchor等保守false标记不提升为更广验收。root协议代码未改，本轮未重跑全部166 headless或三端真实transport。
