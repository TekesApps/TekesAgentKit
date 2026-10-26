# 评价 CAS 写入契约与验证（2026-10-20）

当前分支新增共享反馈规划与原 binding 的一次性 CAS 请求。界面仍为实验状态，评价按钮、备注编辑器尚未接入。历史只读验证收据保持原范围，本轮证据见[JSON 收据](rust-agent-ui-feedback-cas-20261020.json)。

## 状态与动作

`FeedbackChange` 只基于成功读取的 committed record 规划，不乐观覆盖服务器状态。`SetRating` 保留备注，`ToggleRating` 点击同一评价删除评价、切换评价清除旧备注；`SetNote(None)` 只清备注，保留评价。未有评价时不能新建备注；无评价的清除及已无备注的清除是 no-op。编辑器 trim 和空白转 None 属于后续 UI 层；共享协议不另造备注长度限制。

从 `TurnFeedbackTarget::read(section)` 捕获原 scope、turn、公开 message ID 和完整回复内容。重验使用新 session projection：Native 历史 ordinal 改变不换绑同一回复，正文、推理、附件或块变化则拒绝旧目标。展示回退 ID 不能成为反馈目标；有界只读历史不能伪装独立可写会话。

`prepare_feedback` 核对原 binding、publication、连接 epoch、目标内容和当时公开方法，原子认领该 frame；克隆 frame 或并发准备最多接纳一次。no-op 不占用 frame。`FeedbackRequest` 不可克隆，携带固定 RPC ID，`execute(self)` 消费一次，丢弃请求不会发送。调用前后再次核对，runtime 的新 `call_at_epoch` 在 actor 派发处拦截跨重连队列；现有 `call` 行为不变。

结果为 `Applied`、`Conflict`、`Rejected` 或 `NeedsRefresh`。冲突只采用原消息的权威 current（包括明确不存在），不自动重发。已知拒绝保留 committed 前像；运输错误、结果身份/内容不符、畸形冲突或过期完成要求显式刷新。宿主仍须核对挂载/导航状态，不应将旧 pane 的回执显示到新 pane。服务器 version 是不透明 CAS token，只比较相等性，不排序、不要求每次成功都变化；DSH 的同值 put 可以返回原 version。

## 三端协议差异

- Native 删除成功实际返回 `{"deleted":true}`；绑定也接受其他端规范化后的 null，拒绝 false/畸形回执。首次集成因此失败，修正后重跑三端通过。
- Pi 版本冲突改为带 current 的结构化 Remote 错误；成功记录携带 endpoint 生成的真实时间戳并持久化。该时间不是 UI 构造的显示时间。
- DSH 清除 note 时省略 vendor 字段（SDK 不接受 null）；`updatedAt` 统一映射到 `updatedAtMilliseconds`，保留原字段，冲突 current 同样规范化，矛盾时间戳报错。

当前共享范围与 ClientKit 的 rating/note 契约对齐；vendor category 不冒充公共能力。Kernel、TekesUI 和 Swift 实现未修改。

## 验证分层

Root 158、UI workspace 262（含独立公共 API consumer）、脚本 21 tests，以及 root/UI strict Clippy、fmt、依赖边界及固定 TekesUI 输入检查通过。写入边界含 12 个测试，覆盖并发认领、过期帧/回复、重连、延迟结果、权威缺失、已知拒绝及不确定成功后的显式重读。

真实 Native/Pi/DSH 传输使用隔离 session 与本地合成 provider，经过读取、创建、模拟另一调用者引发 CAS 冲突、保存中文备注、保留/清除备注、同值 put、删除及重读确认不存在；两侧 Composer 草稿不变。新增 `--ui-feedback-write` 为显式可选的变更验证，最终清除本次目标评价。这是协议与 offscreen GPU 组合证据，未操作评价控件。

原生 fixture 的六组组件回归通过；系统拼音探针首次在 marked text 后失焦失败，记录保留。单独复测使用相同二进制与断言通过：观察真实 ni hao 组字、Send/导航屏障、空格提交中文及一次显式发送。输入源未修改；物理键盘、候选窗完整体验和系统剪贴板读回未验收。

## 尚未完成

评价按钮/备注编辑器、其余 footnote/entry 能力、可靠 stream→durable 阅读身份，以及完整三端原生 UX 仍需继续。混合内容阅读事务依赖 TekesUI #22，可选中富内容依赖 #25。读取、写入 API 与测试通过不代表完整 Transcript/RunView 或阅读锚点验收通过。UI 保持 0.0.0 / publish=false，不据此合并或打稳定 tag。
