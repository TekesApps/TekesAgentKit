# 评价控件接入（实验，2026-10-20）

本页保留独立控件阶段的验证范围；后续 [Transcript footnote 挂载](rust-agent-ui-feedback-transcript-20261020.md) 已补齐，专项原生评价交互仍待验收。

`TurnFeedback` 是 AgentKit 的独立公开组件，组合 TekesUI 默认按钮和多行文本字段。它拥有评价手势与临时备注编辑，committed records、目标身份、CAS、连接 epoch 和实际调用由现有 presentation / RunBinding 决定。评价按钮横排；正文之外的机器 footnote 区域靠左。颜色、字体、圆角及交互样式继承 TekesUI，没有局部主题副本。

## 输入与回调

1. 从原 binding 的 canonical document 调用 `TurnFeedbackTarget::read(section)`，再 `capture(source.scope())`。每个组件实例绑定一个不可换绑的 capture，namespace 由宿主分配。
2. 未加载时先 `bind_target(capture)`：只允许显式刷新，不把未知记录当作无评价；或先 `read_feedback().await`，将 `frame.controls(&source, &capture)?` 交给 `update`。
3. 每次公开 ContentHost 事件/AX 动作后，立即 `drain_actions` 并交给该组件的 `take_intents`，包括没有按钮动作的文字/组字事件。这会同步按钮的 IME 可用状态。导航宿主在改变 pane、焦点或窗口挂载前检查 `is_composing()`。输入编辑、marked text、选区、撤销和焦点实现仍属于 TekesUI。
4. 在手势原处同步调用 `source.prepare_feedback_gesture(frame.as_ref(), gesture)`；把返回的不可克隆请求交给宿主 executor，勿在异步执行时重新查找当前选中 session。
5. `request.execute().await.apply(&mut original_control)` 返回 `FeedbackUiAdoption`。`Ignored` 保持宿主当前 frame；`Adopted(Some(frame))` 替换 committed frame；`Adopted(None)` 清除旧授权 frame，等待用户显式刷新。回执 token 使用实例身份判断，不能通过复用 namespace 把旧完成投给新控件。克隆手势重复派发直接忽略，不抢先结束真实请求。

组件不拥有网络 executor、RunController 或窗口，不组装 endpoint payload。宿主可将它放在输出 footnote 的布局或宿主提供的面板/popover 中；第二公开 consumer 已验证独立挂载和 gesture bridge API。正式 Transcript 的 per-turn mount 与反馈 frame 分发尚待接入，不能把测试宿主截图当成已接入。

## 备注与错误行为

- 点击同一评价撤销评价，切换评价清除旧备注；未选择评价时备注入口禁用。评价编辑期间禁用其他 rating 操作。
- 备注复用 TekesUI 多行 TextFieldContent，Enter 保留编辑行为，保存必须显式激活按钮。中文组字期间保存、关闭与刷新均不可用。
- 保存 trim 首尾 Unicode 空白及 BOM，空白备注映射成 None；成功关闭编辑器。关闭不提交；保存中仍可关闭，晚到失败显示在原评价行。
- 冲突采用权威 current，保留本地备注草稿；current 明确不存在时关闭编辑器并在行上显示冲突。已知拒绝保留草稿，允许新的显式保存。
- 不确定结果保留已读状态与本地文字，暂停 mutation，只允许显式刷新。刷新恢复 committed state 且保留仍打开的编辑器；若服务器评价已删除则关闭。没有自动写入重试。
- 可见错误使用宿主可配置的 failure/conflict/uncertain 文案；`diagnostic()` 单独暴露内部错误。相同 frame 重发不重建控件；状态刷新复用已有按钮，打开的编辑器保持文本/选区/撤销身份。

## 证据与限制

验证收据见 [JSON](rust-agent-ui-feedback-controls-20261020.json)。UI workspace 270、其中反馈绑定/控件 19 tests，以及脚本 21 tests、UI strict Clippy、fmt 和依赖边界通过；root 源码本轮未改，158 tests 的前轮收据单列保留。组件与原 binding 的测试覆盖组字屏障、显式保存、重复手势、CAS 冲突、权威删除、超时后的显式读取、保存中关闭、旧 mount 回执、只读能力及冷启动读取。较新的 committed frame 可使旧手势失效；保存中权威删除会立即关闭编辑器，旧回执不能覆写新状态。

三端隔离 session 的 opt-in `--ui-feedback-controls` 通过公开 AX 按钮动作和文本提交驱动同一个控件，验证评分、中文备注 trim、空白清除、重开 committed note、同评分删除及删除后重读。浅/深色渲染使用同一组件实例，文本保持不变。这是实际协议与 offscreen GPU 证据，不是原生按钮、物理 IME、候选窗或完整 Transcript 验收。

本组件仍未接入正式 Transcript/RunView 的 per-turn footnote。Native/Pi/DSH 的完整原生 UX、阅读身份迁移、其余 footnote/entry 能力仍需继续；混合阅读事务和富内容分别等待 TekesUI #22/#25。整个 UI workspace 保持 0.0.0 / publish=false。
