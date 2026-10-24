# 旁白 frame 的稳定阅读范围

对照当前 ClientKit `TranscriptMetalActivity.swift` 中稳定的 `narration:<group.id>` 条目，
AgentKit 在 TranscriptContent 中加入真实 NarrationFrame 容器，按当前 view 的 turn/frame 来源分配独立 namespace。
只消费公开 TekesUI `50ec49f3446924fdbc66b3cff22cd8cfe150fe7c`，没有新增 SDK 滚动状态机或像素补偿。
Tekes host 的整体窗口/导航不变，未修改 ClientKit、Tekes、TekesUI 或 Kernel。

## 宿主契约

- `row_id(Message(..))` 返回当前挂载的阅读行，旁白可能是 frame 容器；载体变化时消息 key 可改变，容器 ID 保持。
- `message_id(..)` 返回原始可见 canonical 消息控件；`message_content_ids(..)` 仍指向原正文/推理/工具 part。
- 各原 OutputMessage、文本 owner 与复制来源分别保留，隐藏 peer 不挂载；不把 frame ID 冒充 session pointer 或提交权限。
- frame 根只包装当前正文，不加入间距、颜色或新样式；原 reasoning/tool fragment 仍独立。
- RunPort/ReadOnlyTranscript 的 prepared plan 预测并校验新增 frame namespace；宿主继续通过原 SDK reading transaction 整体发布。
- Activity 折叠时 frame 随原行保留；结束后移出 Activity 仍保持同一正文 span。来源丢失或裁剪不猜补阅读身份。

## 当前验证

完整 UI **606 通过 / 1 原有忽略**，严格 all-target Clippy、格式及 Rust 边界检查通过。
新增 CoreText 测试分别通过第二公开 API consumer 发布 RunPort 与只读 Transcript：
载体切换/恢复保留 frame 锚点，位置误差 ≤1px；Run 保留原正文控件、Composer 焦点/组字和编辑 baseline；
旧 SDK token 被拒绝且不发布；只读 pane 使用自己的 publication，不创建 Composer/反馈权限。
这些属于组件/CoreText 证据，不是原生窗口、候选窗或物理键盘验收。
同一冻结 UI 程序与原 headless（本轮 root 未变）再次通过真实 Pi 1.3.0、DSH 0.2.1-rc.2、
Native Kernel 0.9.3 UI-only 的协议/历史分页/离屏 GPU 路径。原收据中的 complete/rich/native false 均保留；
这条真实协议路径不产生多记录 narration 交叠，不替代专门的载体切换组件 fixture。

冻结程序及原始日志见 `.runtime-fixtures/narration-frame-reading-20261023/`，精确身份见同名 JSON。
上一个程序在用户处理系统提示后仍未获得焦点的失败继续保留，不能转移到此程序，也不能当作成功。
本轮复用共享 target，未新建 target，未删除验收程序/日志/快照。

## 未完成

当前程序的原生窗口与 IME/分页验收、富内容、逐 call 分区和完整 ClientKit 同内容布局仍待完成。
公开 main 仍为当前 pin；#25、#48、#53 仍开放。480433e 的两项核心 CI 成功、
两项 UI CI 均在 #53 的严格 nested-detail 步骤失败，门禁不放宽；PR #10 保持 draft，不合并或打完成 tag。
