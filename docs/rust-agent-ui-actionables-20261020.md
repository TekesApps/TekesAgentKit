# 首批 actionable 内容接入

状态：实验组件，完整等待交互尚未完成。发布基线 AgentKit v0.3.8；TekesUI 继续固定 `431093786de47b48cd566d0d9c0eafa088b10d68`（8 包/188 文件），未修改 Swift、ClientKit、Kernel 或 TekesUI。

只读对照 ClientKit 的 SessionActionablesV3、SessionStreams、respondWithReceipt 和 ApprovalResponseSurface：业务回答保留原 session/actionable/revision，共享层决定待处理状态；UI 只持有局部 pending/error。DSH 的回答信封转换已在前一提交补齐，不放到 UI。

- 新 renderer-free `ActionablePrompt` 根据注册 kind 形成审批“拒绝/仅允许本次”和单个问题的既有单选选项，生成共享 session 回答信封。没有 allowAlways、任意 payload 编辑或默认代答。
- 新公共 `ActionableCard` 复用 TekesUI Stack/Text/Button。输入 publication、epoch、原 prompt 和交互开关；回调保留原 prompt。内容/epoch 改变退休旧 ID，未变化的同步保留按钮及 pointer tracking。原生点击发现并修复了每帧重建按钮导致按压状态丢失的问题，新增回归验证这一点。
- `RunBinding::begin_actionable_response` 再核对 canonical 原始帧与重解析的 choices，拒绝跨源、篡改、过期 revision/epoch；request 一次尝试，仍通过既有 controller/runtime 回传。每个 UI gesture 保留同一 request，不创建多份捕获来重放。
- 局部结果 receipt 属于原卡片，即使切换会话导致卡片保留在树外，晚失败仍回到原卡片；不会改成当前会话的全局错误。错误保持结果未确认/禁用，不自动重试。只在共享 resolved/新源到达时移除/更换。
- ComponentDemo 在 Main 及已有可写 pane 控件组合中挂载；当前宿主逐项显示首个待处理项，其余数量由 Indicators 保留。第二公开 consumer 独立挂载卡片/消费 response request，不导入 demo 或私有控件。

验证：UI workspace 171 tests（含新增 9 项 actionable 和第二 consumer 共 11 项）、root 108 tests、工具 18 tests、UI/presentation all-targets strict Clippy、fmt、UI/headless 依赖边界与 188 输入校验通过。

原生 fixture 使用 `--components --actionable-fixture`：2 项等待 → 点击仅允许本次 → 共享更新到 1 项问题 → 点击既有选项 → 共享移除、空闲。使用真实窗口坐标点击，不执行实际系统权限操作；binary SHA256 `615eee81b3465b66fb0771f82ef3b844e61e81c78eefdebe761c639a61ca1f00`。[AX](rust-agent-ui-actionables-native-20261020.txt)、[问题卡片截图](../ui/docs/actionable-question-native.png)。组字屏障用合成 Preedit 验证，不能称为物理 IME。

尚未支持：多个问题、多选、自定义/纯文本/secret 输入、notes、MCP form/URL、完整显式结果对账/重试 UI、并列待处理列表。此处没有宣称三端各自的 actionable 组件/原生 UI 验收；前一提交真实 DSH 的 headless 提问→回答→tool result 证据仍独立保留。正式 Transcript/RunView、物理中文输入法和阅读锚点验收仍未完成。

全量 171 tests 后，进一步收紧畸形 multiSelect/isSecret 标志和空问题标题的处理：不把未知值当成单选/非 secret。该小增量复跑 9 项 actionable 专项及 UI strict Clippy 通过；原生证据仍以上述 exact binary 为准。
