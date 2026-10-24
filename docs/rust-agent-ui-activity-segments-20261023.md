# 输入边界 Activity 分段

在 `f647100` 基线之上，AgentKit 消费固定公开 TekesUI
`50ec49f3446924fdbc66b3cff22cd8cfe150fe7c`，补齐已结束回合内追加输入的历史分段。
不修改 TekesUI、ClientKit、Tekes 或 Kernel，不引入宿主业务状态机。

## 对齐规则

参照 ClientKit `TranscriptMetalActivity.swift` 的 `collapseBetweenInputs`：

- 在原连续工具分组完成后，以 canonical 输入身份切段；输入始终可见，最终正文独立保留。
- 非空前段为 `Previous 1 message` / `Previous N messages`；计数是语义条目，不能按工具成员、结果或渲染节点数累计。
- 连续输入之间不创建空 Previous 行；末段总有结束状态，空末段是被动标签。
- 分段全部默认折叠，包含异常结束；普通整回合的异常默认展开规则保持独立。
- 隐藏时保留原 OutputMessage/fragment/tool 控件；本地展开选择使用原 generation 门禁、整帧 preflight 与 TekesUI reading transaction。

公开 API 为 `TranscriptActivitySegmentKey { turn_id, end }`、
`TranscriptActivitySegmentEnd::{BeforeInput, Final}` 和三种 owning view 上的
`set_activity_segment_expanded`。`activity_previous` 文案由宿主回调提供。
未安装、空的被动段及外部段均拒绝；只读 pane 的 publication 仍独立。
示例宿主只转交 `TranscriptLayoutChanged` 并重新发布已捕获快照，不创建 endpoint 请求。

## 本轮验证身份

冻结 UI 程序 SHA-256：`cb3b3aa319db68c9bc8859654c4f26ec7cb42e0eed6ec7f4aa5dfcd76897c362`。
程序、差异、变更源码、日志和收据保存在
`.runtime-fixtures/activity-segments-20261023/final/`，摘要见同名 JSON。
不将此前 `c90c9f3b` 程序的收据转移到本轮。

- UI workspace **600 通过、0 失败、1 项原有忽略**；严格 all-target Clippy、构建与静态边界检查通过。
- 第二 public-API consumer 覆盖可写 RunPort 与只读历史；组件测试验证语义计数、默认值、稳定输入键、旧计划拒绝和控件身份。
- CoreText 阅读事务用例实际执行，验证展开时向上阅读锚点、焦点和中文 preedit 保留。
- 本地公开 journal fixture 的原生普通/分段 profile 均通过实际控件点击、工具与推理展开、外层收起/再展开、resize/type-scale 重排及原文本 owner 保留。marked text 和 wheel 是测试事件；不是物理键盘、拖动或真实 harness 证明。
- 同一程序一次 LaunchServices 系统软件拼音测试通过，PID 定向事件，输入源前后均为系统拼音；不代表 IME 稳定性、候选窗口、物理键盘或 VoiceOver 已验收。
- 同一程序的 Pi 1.3.0、DSH 0.2.1-rc.2、Native Kernel 0.9.3 原生 stream、stream-reading 与公共分页 profile 全部通过。三端使用真实 harness 和本地合成 HTTP provider；分页数量分别 50→56、50→59、50→56，`nativePagingControl=false`。原始 false flags 保留在 JSON，不能从此推导完整 RunPort/Transcript、分页按钮或全量焦点组合已通过。Native 为显式 UI-only，不含完整生命周期/有界子历史。
- 根源码、manifest、lock 未变；沿用上一阶段根 289 项及 root Clippy 的独立证据，不重复宣称本轮重跑。

## 限制

初始 prompt role、剩余 narration 来源差异、完整富内容与 ClientKit 同内容视觉对照仍未完成。
正式同/跨进程 ProcessContainer 等待 TekesUI #48，严格嵌套详情阅读 #53 仍失败。
PR #10 继续为试验状态，未合并、未打 tag。此次复用现有 target，未创建独立 target；
冻结可执行文件采用 hardlink，保留必要源码、日志与证据，未删除用途不明的共享缓存。
