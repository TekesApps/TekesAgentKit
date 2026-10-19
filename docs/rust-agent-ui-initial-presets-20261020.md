# 初始预设的公开选择与创建适配

基线仍为已发布 AgentKit v0.3.8，TekesUI 固定公开
`431093786de47b48cd566d0d9c0eafa088b10d68`；八个包、188 输入不变。
本次 API 属于 PR #10 未发布增量，没有修改 Swift、Kernel 或 TekesUI。

只读对照当前 ClientKit 的 `SessionInitialPresets.swift`、
`SessionEndpointRuntime.agentPresetCatalog` 和 DSH InitialPresets adapter。
初始预设是在创建时原子应用的 Harness 配置；即使已有会话仍 blank，也保留原选择。
因此这次不把 DSH 的旧 presets.select 当作统一的现有会话切换入口。

`InitialPresetChoices` 读取已声明的公共 session.initialPresets；DSH 使用
presets.list roster。加载核对连接 epoch，不将读取失败变成空目录或改用缓存。
原始配置/扩展保留，title/name、description、broken/unavailableReason 与 default
字段适配为已有 ComposerChoices。损坏项禁用，重复/畸形/冲突 default 拒绝。
default 是 Harness 元数据，不是用户手势；未选择时 agent_preset=None 继续保留自动行为。
configurationText 不放进草稿正文、按钮名称或模型指令。

组件选择捕获原 request/destination/catalog revision/choice。create_request 校验
NewThread、原目录项、workspace 与宿主 preset 冲突，只设置已有 CreateSessionRequest
的 agent_preset。revalidate_create 在真正创建前重新读取公开目录；丢失、禁用或变更的
choice 不自动替换为默认项。宿主仍需围绕 await 核对当前手势/导航、匹配 prepared
ticket 的 destination，在验证失败时 cancel_prepared 恢复已冻结草稿，并保留不确定
创建的原 session/RPC 身份。controller 继续拥有创建 materialize、单次发送和回传。
这不是新增一套 Run 状态机，路径和权限仍由宿主明确输入。

第二公开 consumer 提供 initial_preset_panel，只消费两个公开 UI 包。测试宿主通过
公开 ComposerChoices AX 选择，接入 RunController 创建和单次发送；覆盖两种目录词汇、
禁用/双击、错误目录、跨端点、已有会话、过期/伪造 choice，以及不注入配置正文。
本地 UI 105 tests、严格 all-targets Clippy、fmt、demo build、工具 18 tests 通过。
root 实现未改，本轮没有重复其上一增量 105 tests。

`--components --initial-preset-proof --initial-preset-cwd ABSOLUTE_PATH --offscreen PNG`
是脚本专用证明路径；用公开选择、创建参数、ModelSelectionPreparation 与共享 controller
执行额外一条本地合成 provider 消息。三端运行记录、失败及最终结果见
[机器收据](rust-agent-ui-initial-presets-20261020.json)。它不构成交互式 NewThread 入口、
原生预设面板像素或物理 IME 验收；GPU 截图仍是主 component demo 的结算后画面。

首轮 Pi 成功；DSH 创建/发送成功后，验证错误地只在 inventory summary 查找预设身份而
失败。根据其公开 agentPreset projection 改正读取位置，保留首轮失败记录。
Pi 的固定 default 目录允许显式创建，但不发布独立预设身份字段，收据不声称该字段回传。

尚需交互式新会话/初始预设选择接入及原生验证；实际附件选择/上传/预览、批注编辑与
来源高亮、内容 undo、其余 Composer 能力入口仍未完成。正式 Transcript/RunView 等待
TekesUI #22/#23 公开输入，完整 SidePanel 内容、物理 IME、阅读与分页锚点验收仍待完成。
RunSurface/ComponentDemo 保持测试宿主身份，PR #10 仍为 draft，未合并/tag。
