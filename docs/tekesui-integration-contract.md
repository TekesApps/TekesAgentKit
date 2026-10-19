# AgentKit × TekesUI：首个 Run 接入契约

状态：开发试点，非生产 UI 发布。范围为连接状态、纯文本 Transcript、基础 Composer、显式发送/停止、流式回传、会话草稿切换、重连和分页。初始提交为单行 Composer；当前开发候选消费公共多行视口，Enter 换行，发送策略不变。Tasks/Subagents/SideChat/Summary 暂不接 UI。

## 依赖与构建

- AgentKit 当前基线 v0.3.7 / a8610a420bad72eaedcf706b30df87153a0f3e16，Rust 1.95.0。已合入 WebUI 使用的批注、线程操作/置顶、用量与完成回合 Fork 契约；本 UI 的挂载范围仍为首段 Run 闭环，不把这些共享能力等同于控件已接入。
- WebUI 对照版本 v0.1.17 / 8b9b11c；仅保留既有 Fork，不加入已撤回的回退/编辑重跑实验。Native/Pi/DSH 显式配置与分层验收见 [三端记录](tekesui-three-endpoint-demo-20261019.md)。
- TekesUI composition仍为实验API；源码固定为公开 `https://github.com/TekesApps/TekesUI.git` 的完整rev `431093786de47b48cd566d0d9c0eafa088b10d68`，Cargo.lock固定同一来源。188个输入与已验收037相同，清单见 `ui/tekesui-source.json`。
- 默认由Cargo取得公开Git输入，不读取活跃兄弟工作树或旧 `.deps`。开发快照准备/capture在公开pin模式禁用；旧e4依赖和回滚文件保留。UI workspace与headless默认构建隔离。
- 已具备公开不可变源码pin；AgentKit UI仍为0.0.0/publish=false，不打UI release tag。验收范围和CI边界见 [正式pin记录](tekesui-public-pin-verification-20261019.md)。

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
- 首版只有显式 Send，Enter 永不提交。marked text 存在时禁止 Send 和会话切换；不在 AgentKit 内实现候选窗、组字算法或新撤销栈。原始多行键盘编辑需求见清单；当前候选的独立自动化结果与原生未验收边界见 [复验记录](tekesui-consumer-verification-20261018.md)。
- 发送回填、草稿恢复和晚回包归属继续由现有 presentation 管理。取消网络任务不等于取消本地编辑器组字。

## 阅读与历史

- Transcript 保留只读 TextFieldContent 的输入/选区语义，使用其公开 TextSurface + ReadingController 绘制完整阅读视口。向上滚动/选择后停止跟随；尾部追加与重排通过现有阅读锚点保留位置；只有显式“回到最新”恢复跟随，不抢 Composer 焦点。
- 每个 destination 保留自己的 Transcript 控件/阅读状态。消息内容来自现有 projector，不直接读取 harness 文件。
- Earlier 调用已有冻结分页；AgentKit 按稳定 TranscriptItem.id 提供保留文本的 UTF-16 区间映射，ReadingController 原子更新文档并恢复阅读锚点。重复 ID 不映射，流式追加只映射双方字素边界内的公共前缀；连续保留段合并，删除或变化部分遵循上游显式失效/fallback。每 destination 保留阅读实例；UI 只从 controller 最新 snapshot 同步，不直接套用异步历史回包。

## 验证边界

UI demo 的默认 transport 是本地、确定性的 SEP fixture，经过真实 SessionRuntime/RunController；不是另一个 UI 状态机，也不代表真实模型验收。原有 Native/Pi/DSH CLI 集成独立保留。验证分协议与状态、公共组件消费、原生合成文本服务、真实物理 IME/VoiceOver/性能五层，后两层不能由自动合成输入替代。

后台更新通过现有 controller watch 订阅转成 WindowWake；操作完成先将结果放入宿主队列再 wake。窗口关闭后终止订阅；代理不持有窗口。控件动画按需开启/停止，不轮询业务数据；原生 AgentKit 最终体验仍须单独验收。

Demo 支持显式 Native endpoint JSON（URL、bearer 环境变量名、两个 session ID）；真实 Native SEP 已验证到 RunController → TekesUI GPU 离屏输出。该证据不代表真实模型或原生交互验收。运行方法见 [UI README](../ui/README.md)。

## 指针接入更新

#19最新WIP已接入原生button/modifiers/click_count；AgentText包装器转交pointer_event，RunSurface的组字屏障覆盖完整指针入口。双击沿原Unicode分词选择实际命中字素，三击逻辑段落。旧简化事件兼容；详见 [分层证据与迁移](tekesui-pointer-metadata-20261018.md)。原生AgentKit窗口尚未验收，正式pin仍阻塞。
