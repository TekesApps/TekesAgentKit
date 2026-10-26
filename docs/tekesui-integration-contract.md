# AgentKit × TekesUI：首个 Run 接入契约

状态：开发试点，非生产 UI 发布。当前阅读接入与三端恢复见[最新记录](rust-agent-ui-retained-reading-20261021.md)；以下RunSurface行为不作为正式组件完成声明。本文件保留首个 RunSurface 协议/阅读测试宿主的契约，不是正式 Transcript/RunView 的组件规格。当前独立组件、任务/子代理/SideChat/Summary 与 Sidebar 操作进度以 [Rust Agent UI 契约](rust-agent-ui-contract.md) 为准。首段闭环覆盖连接、纯文本历史测试、Composer、显式发送/停止、流式、草稿切换、重连和分页；Composer 消费公共多行视口，Enter 换行。

## 依赖与构建

- AgentKit 当前基线 v0.3.8 / f1daa0ffcf6ced41e21f351e22845455ca78006c，Rust 1.95.0。已合入 WebUI 使用的批注、线程操作/置顶、用量与完成回合 Fork 契约；共享能力和具体控件的接入状态分别记录在正式契约中。
- WebUI 对照版本 v0.1.17 / 8b9b11c；仅保留既有 Fork，不加入已撤回的回退/编辑重跑实验。Native/Pi/DSH 显式配置与分层验收见 [三端记录](tekesui-three-endpoint-demo-20261019.md)。
- TekesUI composition仍为实验API；源码固定为公开 `https://github.com/TekesApps/TekesUI.git` 的完整rev `50ec49f3446924fdbc66b3cff22cd8cfe150fe7c`，Cargo.lock固定同一来源。实际八包243文件输入；原431/8cf/2ee基线保留，清单见 `ui/tekesui-source.json`。
- 默认由Cargo取得公开Git输入，不读取活跃兄弟工作树或旧 `.deps`。开发快照准备/capture在公开pin模式禁用；旧e4依赖和回滚文件保留。UI workspace与headless默认构建隔离。
- 已具备公开不可变源码pin；AgentKit UI仍为0.0.0/publish=false，不打UI release tag。验收范围和CI边界见 [正式pin记录](tekesui-public-pin-verification-20261019.md)。

## 所有权与数据方向

| 层 | 拥有 | 不承担 |
|---|---|---|
| TekesUI | 通用输入/选区/撤销/组字、控件焦点、布局/渲染、ReadingController/滚动几何、平台文本服务 | Session、发送回执、连接状态、Agent 草稿归属 |
| AgentKit presentation | Session scope、RunSnapshot、草稿、提交身份/echo、取消、重连后的状态、历史分页 | 原生编辑与滚动的另一个实现 |
| AgentKit UI components | Agent 业务控件组合、每 destination 的控件实例、Agent 意图映射、只读 snapshot 展示；RunSurface 仅作测试宿主 | SEP 协议、重试策略、第二套 Run 状态机 |
| Demo host | 窗口/主题输入、Tokio 执行器、公开平台事件翻译、Clipboard/AX/TextInputState 桥接、transport 配置 | 私有 AppKit 接口、TekesUI 缺失能力的通用替代实现 |

输入是 `RunSnapshot`；输出是捕获 destination 的 Edit、Send、Cancel、Earlier、Reconnect、Select 意图。
Edit 调用 RunController::edit；Send 在发起时 prepare 捕获 ticket，再异步 dispatch；Cancel、Earlier 分别调用 controller 原接口；Reconnect 调用 SessionRuntime。错误可见，不重放未知写操作。连接状态与按钮权限只从 snapshot 派生。

## 焦点、IME 与草稿

- 每个 destination 保留自己的 TekesUI 编辑器实例，切换不按帧重建控件；同一 scope 的异步数据更新不抢焦点、不覆盖编辑器选区/undo/preedit。
- 组字由 TekesUI TextFieldContent/TextInput 处理。preedit 不写入 Agent 草稿；Commit/Unmark 后才发出 Edit。
- 首版只有显式 Send，Enter 永不提交。marked text 存在时禁止 Send 和会话切换；不在 AgentKit 内实现候选窗、组字算法或新撤销栈。原始多行键盘编辑需求见清单；当前候选的独立自动化结果与原生未验收边界见 [复验记录](tekesui-consumer-verification-20261018.md)。
- 发送回填、草稿恢复和晚回包归属继续由现有 presentation 管理。取消网络任务不等于取消本地编辑器组字。

## 阅读与历史

以下是 RunSurface 单文本 History 的阅读 API 测试路径，不满足正式 Transcript 的 input 气泡、output/activity 与 footnote 分层，也不能证明混合文档分页锚点。正式组件使用上述 Rust 契约，缺失的通用能力通过 TekesUI #22/#23 交付。

- Transcript 保留只读 TextFieldContent 的输入/选区语义，使用其公开 TextSurface + ReadingController 绘制完整阅读视口。向上滚动/选择后停止跟随；尾部追加与重排通过现有阅读锚点保留位置；只有显式“回到最新”恢复跟随，不抢 Composer 焦点。
- 每个 destination 保留自己的 Transcript 控件/阅读状态。消息内容来自现有 projector，不直接读取 harness 文件。
- Earlier 调用已有冻结分页；AgentKit 按稳定 TranscriptItem.id 提供保留文本的 UTF-16 区间映射，ReadingController 原子更新文档并恢复阅读锚点。重复 ID 不映射，流式追加只映射双方字素边界内的公共前缀；连续保留段合并，删除或变化部分遵循上游显式失效/fallback。每 destination 保留阅读实例；UI 只从 controller 最新 snapshot 同步，不直接套用异步历史回包。

## 验证边界

UI demo 的默认 transport 是本地、确定性的 SEP fixture，经过真实 SessionRuntime/RunController；不是另一个 UI 状态机，也不代表真实模型验收。原有 Native/Pi/DSH CLI 集成独立保留。验证分协议与状态、公共组件消费、原生合成文本服务、真实物理 IME/VoiceOver/性能五层，后两层不能由自动合成输入替代。

后台更新通过现有 controller watch 订阅转成 WindowWake；操作完成先将结果放入宿主队列再 wake。窗口关闭后终止订阅；代理不持有窗口。控件动画按需开启/停止，不轮询业务数据；原生 AgentKit 最终体验仍须单独验收。

Demo 支持显式 Native endpoint JSON（URL、bearer 环境变量名、两个 session ID）；真实 Native SEP 已验证到 RunController → TekesUI GPU 离屏输出。该证据不代表真实模型或原生交互验收。运行方法见 [UI README](../ui/README.md)。

## 指针接入更新

#19 的指针能力已纳入上文固定公开 pin；AgentText 包装器转交 pointer_event，RunSurface 的组字屏障覆盖完整指针入口。双击沿 Unicode 分词选择实际命中字素，三击逻辑段落。旧简化事件兼容；历史证据见 [分层证据与迁移](tekesui-pointer-metadata-20261018.md)。当前局部原生合成文本服务验证见正式契约，物理 IME 和正式 Transcript 阅读验收仍未完成。

2026-10-21 开发候选补充：固定 #34 的实际 Composer IME 门禁、成功render后绑定、重挂重绑、折叠/恢复及主/SideChat原生合成组字证据见[接收契约](rust-agent-ui-padding-ime-20261021.md)。只有公开精确pin具备这些接口后，才可去除隔离适配；默认8cf0840仍保持原门禁，查询结果不得缓存或解释为整块输入胶囊的通行许可。
