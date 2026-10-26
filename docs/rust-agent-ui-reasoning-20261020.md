# 模型推理强度接入（实验状态）

AgentKit 发布基线仍为 v0.3.8，TekesUI 固定公开 `431093786de47b48cd566d0d9c0eafa088b10d68`，8 包 / 188 输入校验未变。未修改 Swift、ClientKit、Kernel 或 TekesUI。

只读对照 ClientKit 的 `ComposerModelCatalogRead.swift`、`ComposerModelPanel.swift`：档位与当前/默认推理选择来自模型目录，不只比较模型标题。公共 `ModelChoices::reasoning_choices` 按完整 provider/model 身份读取 reasoning.efforts/defaultEffort，保留未知当前档位、目录顺序和禁用原因；不补造默认档位。Pi 仅为当前模型提供 reasoning 元数据，缺少时保持不可用。

公共 `ReasoningChoices` 输出既有 `ComposerChoiceCatalog` 和解析后的模型设置 payload；`ComposerChoices` 增加 Reasoning kind 与可本地化标题，继续复用 TekesUI Stack/Text/Button。没有新菜单、编辑器或 Run 状态机。第二公开 consumer 提供 `reasoning_panel`。

ComponentDemo 的模型入口同时挂载模型及该模型的推理内容。选择只经原 binding 的既有 settings request 修改草稿，发送时应用。任一选择/关闭会同时收起两块内容，导航与组字继续使用既有屏障；已换草稿模型时，旧 reasoning 回调不能把它切回旧模型。它仍是公共组件验证宿主，完整弹层及正式 RunView 尚未完成。

发送前重新读取公开 session.models，并在调用 session.selectModel 前核对显式 reasoningEffort；档位删除、未公开或不可用时失败并走既有草稿恢复，不自动重试。不是把 provider 配置拼进 prompt。

验证范围：

- 新增 5 项测试：目录/未知选择/缺能力、坏元数据、不虚构默认、实际 UI 回调到 settings/Send 顺序、无效档位在 mutation 前拒绝、组字/导航/换模型失效；第二 consumer 新增独立挂载测试。合成 Preedit 不是物理输入法证据。
- UI workspace 158 tests、第二 consumer 10 tests（包含在总数中）、18 项工具测试、UI all-targets strict Clippy、fmt、静态依赖边界和精确固定输入检查通过。
- 真实三端 + 隔离合成服务回归：Native 公开按钮选择 `low`、Pi 公开按钮选择 `off`，各有配置回执并完成一次 prompt；本轮 DSH 模型未公开档位，入口保持不可用。Pi 的隔离配置只在 `--ui-model-choices` 下标记测试模型支持 reasoning；不修改用户配置或私有数据库。
- 本地 fixture 原生窗口可见模型与推理组件、配置默认 low，选择 high 后重新打开可见 high 选中；B 不携带 A 的设置，返回 A 保留草稿设置。原生证据来自真实按钮坐标交互；没有物理中文 IME、VoiceOver 或三端各自原生窗口验收。

[机器收据](rust-agent-ui-reasoning-20261020.json)、[原生 AX](rust-agent-ui-reasoning-native-20261020.txt)、[组件截图](../ui/docs/reasoning-options-native.png)。

仍未交付正式 Transcript/RunView；TekesUI #22/#23 的公共能力阻塞保持不变。来源选区/高亮、图片/历史附件预览、失败对账和完整 Agent 设置仍需继续完成，不能据此合并或打完成 tag。

增量复查：模型目录现与初始预设读取一致，在公开读取前后核对连接 epoch 与 Ready 能力；宿主目录手势也捕获 epoch。重连后旧成功/错误不能重开面板或关闭较新的手势，已挂载的模型/推理按钮会退休。新增 3 项 `model_reconnect` 用例通过；同轮模型/推理回归 8 项、第二公开 consumer 10 项及 strict Clippy 通过。这里是针对连接失效的增量证据，未重跑三端/原生窗口，沿用前述独立证据及边界。
