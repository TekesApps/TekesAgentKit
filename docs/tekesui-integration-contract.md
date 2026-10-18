# AgentKit × TekesUI：首个 Run 接入契约

状态：开发试点，非生产 UI 发布。范围为连接状态、纯文本 Transcript、单行 Composer、显式发送/停止、流式回传、会话草稿切换、重连和分页。Tasks/Subagents/SideChat/Summary 暂不接 UI。

## 依赖与构建

- AgentKit 基线 v0.3.0 / aab9fb30623525a949b6fae80af3646a8c5f2ece，Rust 1.95.0。
- TekesUI 的 composition 仍为实验 API。当前消费 base commit 及完整源码 SHA-256 见 `ui/tekesui-source.json`，包含该工作树必要的未提交实现；不能仅用 base commit 重现。
- `prepare-tekesui-demo.py --source <明确选择的源码>` 只读取外部工作树，复制并校验清单文件到被忽略的 `ui/.deps/tekesui`。不修改、不提交、不推送 TekesUI；不静默跟随 sibling path。UI workspace 与 headless 默认构建隔离。
- 这是开发快照，不是发布依赖。发布前必须取得包含这些 API 的不可变公开 commit/tag，再转换依赖；当前不打 UI release tag。

## 所有权与数据方向

| 层 | 拥有 | 不承担 |
|---|---|---|
| TekesUI | 通用输入/选区/撤销/组字、控件焦点、布局/渲染、ReadingController/滚动几何、平台文本服务 | Session、发送回执、连接状态、Agent 草稿归属 |
| AgentKit presentation | Session scope、RunSnapshot、草稿、提交身份/echo、取消、重连后的状态、历史分页 | 原生编辑与滚动的另一个实现 |
| AgentKit UI components | RunSurface 组合、每 destination 的控件实例、Agent 意图映射、只读 snapshot 展示 | SEP 协议、重试策略、第二套 Run 状态机 |
| Demo host | 窗口/主题输入、Tokio 执行器、公开平台事件翻译、Clipboard/AX/TextInputState 桥接、transport 配置 | 私有 AppKit 接口、TekesUI 缺失能力的通用替代实现 |

输入是 `RunSnapshot`；输出是捕获 destination 的 Edit、Send、Cancel、Earlier、Reconnect、Select 意图。
Edit 调用 RunController::edit；Send 在发起时 prepare 捕获 ticket，再异步 dispatch；Cancel、Earlier 分别调用 controller 原接口；Reconnect 调用 SessionRuntime。错误可见，不重放未知写操作。连接状态与按钮权限只从 snapshot 派生。

## 焦点、IME 与草稿

- 每个 destination 保留自己的 TekesUI 编辑器实例，切换不按帧重建控件；同一 scope 的异步数据更新不抢焦点、不覆盖编辑器选区/undo/preedit。
- 组字由 TekesUI TextFieldContent/TextInput 处理。preedit 不写入 Agent 草稿；Commit/Unmark 后才发出 Edit。
- 首版只有显式 Send，Enter 永不提交。marked text 存在时禁止 Send 和会话切换；不在 AgentKit 内实现候选窗、组字算法或新撤销栈。多行键盘编辑缺口见清单。
- 发送回填、草稿恢复和晚回包归属继续由现有 presentation 管理。取消网络任务不等于取消本地编辑器组字。

## 阅读与历史

- Transcript 保留只读 TextFieldContent 的输入/选区语义，使用其公开 TextSurface + ReadingController 绘制完整阅读视口。向上滚动/选择后停止跟随；尾部追加与重排通过现有阅读锚点保留位置；只有显式“回到最新”恢复跟随，不抢 Composer 焦点。
- 每个 destination 保留自己的 Transcript 控件/阅读状态。消息内容来自现有 projector，不直接读取 harness 文件。
- Earlier 调用已有冻结分页；当前 ReadingController 对头部插入不提供消息 ID 锚点映射。分页数据正确性可验证，但头部插入后的无跳动体验为阻塞缺口，不能标为通过。

## 验证边界

UI demo 的默认 transport 是本地、确定性的 SEP fixture，经过真实 SessionRuntime/RunController；不是另一个 UI 状态机，也不代表真实模型验收。原有 Native/Pi/DSH CLI 集成独立保留。验证分协议与状态、公共组件消费、原生合成文本服务、真实物理 IME/VoiceOver/性能五层，后两层不能由自动合成输入替代。

后台 UI 唤醒缺口使 demo 必须显式启用 `--frame-pump`；它仅是开发演示驱动，不是生产解决方案，不声称空闲零回调。缺口及所需上游接口见 [清单](tekesui-gaps.md)。

Demo 支持显式 Native endpoint JSON（URL、bearer 环境变量名、两个 session ID）；真实 Native SEP 已验证到 RunController → TekesUI GPU 离屏输出。该证据不代表真实模型或原生交互验收。运行方法见 [UI README](../ui/README.md)。
