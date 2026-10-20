# Rust Run demo（开发试点）

只消费 TekesUI，复用 AgentKit v0.3.8 的 `SessionRuntime` / `RunController` / `RunSnapshot`。
组件层不实现另一套 Run 状态机。默认 headless workspace 和发布 API 保持独立。

[短接入契约](../docs/tekesui-integration-contract.md) 定义所有权、输入、回调、焦点、IME 和阅读责任；
[TekesUI 缺口](../docs/tekesui-gaps.md) 列出所需上游事件/属性，
[七项开发任务](../docs/tekesui-development-tasks.md) 给出优先级与逐项验收标准。

![基础 Run 离屏渲染，非原生窗口截图](docs/run-offscreen.png)

## 正式组件开发

`RunSurface` 仅是现有协议/控件测试宿主，不是标准 RunView。正式组件契约、App 的 input/output/footnote 布局、基础 Issue 和完整交付账本见 [Rust Agent UI 接入契约](../docs/rust-agent-ui-contract.md)。

可独立挂载的组件包括 Composer、ComposerContext、ComposerChoices、AnnotationEditor、ComposerUploads、Indicators、Sidebar、ThreadRename、SidePanel、SummaryView、InputMessage、OutputMessage、TurnFootnote、TranscriptContent、ReadOnlyTranscript 和 RunView。它们使用同一套共享 presentation；RunSurface/ComponentDemo 是测试与接入宿主。主会话和可写 pane 已挂载 RunView，有界子历史使用 ReadOnlyTranscript；完整 Transcript 的阅读锚点、富内容与最终体验验收仍待完成。

可选的[系统拼音软件探针](../docs/rust-agent-ui-system-ime-20261020.md)使用 `python3 scripts/test-system-ime.py --demo ui/target/debug/tekes-agent-ui-demo`（在仓库根目录执行）。它要求当前已选简体拼音和已有事件权限，不改变输入源，只向自己启动的 fixture 进程发送键码；通过真实 marked text、发送/导航屏障、确认和单次提交检查。实体键盘、候选窗视觉和完整 IME 验收仍未完成。

Composer 已接入输入/草稿/发送/取消、公开模型目录与发送前配置、初始预设/自动创建、内容卡片移除和已有批注评论编辑。Main 已接入真实 macOS 文件选择和公共上传状态；三端公开上传→草稿→发送→历史/重连已验证，Native/Pi 另验证通用文件字节读取，DSH 暂无通用文件下载。SidePanel 选择器已接入任务、独立子代理和 SideChat；预览/持久化对账、选区批注捕获/来源高亮、内容移除 undo 和其余能力入口仍待适配。模型、创建和评论编辑的证据分别见 [模型闭环](../docs/rust-agent-ui-model-integration-20261019.md)、[新会话](../docs/rust-agent-ui-creation-20261020.md)、[评论编辑](../docs/rust-agent-ui-annotation-editor-20261020.md)。

Sidebar 使用共享列表、置顶和动作 dispatcher，已接入选择、复制、重命名、分叉和归档。SidePanel 组合任务、子代理、SideChat 与 Summary；任务与独立子代理保留各自 Composer/Indicators，有界子历史是只读来源。SideChat 保留临时 pointer 与 Unknown/Closing 草稿，源撤销、晚结果和异步准备继续由原绑定检查。

[绑定包](bindings/README.md) 连接原 RunController、history lease、任务/子代理/SideChat 生命周期。宿主拥有窗口、布局、导航与系统能力；来源替换不能重定向旧回调。第二 public consumer 只依赖两个公开 UI 包。

`--components` 打开实际组件验证窗口，可与 `--offscreen` 或显式 `--endpoint-config` 组合。原生 fixture 的 `--native-selftest` 及各单项模式验证合成文本服务、发送/取消/重连、草稿与 pointer 隔离；真实三端离屏接入使用 `--ui-components`。物理 IME、正式 Transcript 阅读/分页锚点、完整宿主和三端原生交互仍需验收。最新范围与收据以 [交付账本](../docs/rust-agent-ui-contract.md) 为准。

## 依赖

- Rust 1.95.0、macOS + Metal（原生窗口/文本服务/GPU）；Python 3.11+（准备和边界检查）。
- AgentKit 基线 v0.3.8（公开 main `f1daa0ffcf6ced41e21f351e22845455ca78006c`）；Native 真实集成基线 Kernel v0.9.1。
- TekesUI composition 仍是实验 API，但实验开发源码固定为公开 Git commit `8cf0840790334ba224afc0595897c74231c712e6`；6 个直接依赖和2个传递依赖均由 Cargo 按该完整 rev 取得，ui/Cargo.lock 同步固定。
- `tekesui-source.json` 的 kind 为 `published-git-revision`，199文件 SHA-256 为 `f0af230a1efe97334b90eb193f77ca0d30dec43ef09e487ad6948c38a938257f`，对应实际八包依赖闭包；原431基线在 `baselines/` 原样保留。固定的是不可变commit，不跟随分支HEAD。
- 旧e4 manifest、lockfile、消费适配和188依赖另有备份；旧 `.deps/tekesui` 原样保留但不参与当前构建。无需从兄弟工作树复制代码，`prepare-tekesui-demo.py` 在公开pin模式拒绝path/capture。
- UI包继续 `0.0.0` / `publish=false`；公开源码pin不代表AgentKit UI稳定版。最新验收与独立CI边界见 [正式pin复验](../docs/tekesui-public-pin-verification-20261019.md)。

## 构建与运行

从 AgentKit 根目录：

```sh
python3 scripts/check-ui-boundaries.py
cargo build --manifest-path ui/Cargo.toml --locked -p tekes-agent-ui-demo
ui/target/debug/tekes-agent-ui-demo
```

默认 transport 是独立的本地 SEP fixture：两个会话、有界历史、逐块回传和可取消运行，不需要凭证或模型。
按钮提供发送、停止、历史分页、回到最新、重连、切换 A/B。候选 Composer 已消费公共多行 Grow 视口；Enter 换行，业务发送仍仅通过显式按钮。
宿主订阅已有 `RunController::subscribe()`，通过公开 `WindowWake` 投递通知；异步操作结果入队后同样 wake。没有业务 frame pump；原控件的有限过渡仍按需请求动画帧。

真实 Native、Pi、DSH 都通过 `--endpoint-config` 接收宿主显式 JSON。A/B 对应所选 endpoint 的两个已有 session，必须互不相同；宿主负责模型/执行参数。demo 启动不创建真实会话或修改模型；`--components` 中显式点击新建 SideChat 会在支持的 endpoint 创建临时 fork，关闭操作会取消并丢弃该临时会话：

```json
{"endpointId":"native-ui","url":"http://127.0.0.1:PORT","bearerEnv":"MY_EXPLICIT_BEARER","sessions":["SESSION_A","SESSION_B"]}
```

```sh
ui/target/debug/tekes-agent-ui-demo --endpoint-config /absolute/native.json
```

Native 可显式设置 `"kind":"native"`；省略 kind 兼容旧配置。令牌仅从 `bearerEnv` 指定的环境变量读取。

Pi 1.3.0：

```json
{
  "kind":"pi", "endpointId":"pi-ui", "sessions":["SESSION_A","SESSION_B"],
  "executable":"/absolute/pi-runtime/node/bin/node",
  "arguments":["/absolute/pi-runtime/node_modules/@earendil-works/pi-coding-agent/dist/bundle/cli.js"],
  "dataRoot":"/absolute/owned-agentkit-pi-data",
  "environment":{"HOME":"/absolute/authorized-home","PATH":"/usr/bin:/bin"},
  "environmentFromEnv":{"PROVIDER_API_KEY":"HOST_PROVIDER_KEY"},
  "extensions":[], "sessionRoots":[]
}
```

Pi 复用同一个 AgentKit dataRoot 的 session registry/pointer，且该目录同时只能被一个 connector 持有。
`executable`、`dataRoot`、`extensions`、`sessionRoots` 必须为绝对路径。`sessionRoots` 仅授权现有 adapter 采用该范围内的 session 文件，不扫描全局目录。
子进程不继承宿主环境；普通输入放在 `environment`，敏感值以 `environmentFromEnv` 的“子进程变量名 → 宿主变量名”显式映射。两者不可写同一目标变量。
provider/model、代理及证书等运行所需输入均由宿主配置。

DSH 0.2.1-rc.2（兼容 0.2.0-rc.2）：

```json
{"kind":"dsh","endpointId":"dsh-ui","url":"http://127.0.0.1:PORT","cookieEnv":"HOST_DSH_COOKIE","sessions":["SESSION_A","SESSION_B"]}
```

可将 `cookieEnv` 替换为 `bearerEnv`，不能同时设置；显式无认证的宿主可省略两者。
沿用 DSH connector 的官方运行版本探测，宿主需提供 `/api/tekes/harness-version` 只读桥接，不能用配置伪造版本。
三个 backend 共用现有 SessionRuntime / RunController 和同一个 RunSurface，发送使用已有 session 的实际模型配置。
未知 kind/字段、错误 session 槽位、缺失凭证变量均报错，不回退 fixture。通用 TekesUI 控件与状态逻辑保持上游所有。

## 验证

```sh
cargo test --manifest-path ui/Cargo.toml --workspace --locked
cargo clippy --manifest-path ui/Cargo.toml --workspace --all-targets --locked -- -D warnings
ui/target/debug/tekes-agent-ui-demo --native-selftest
ui/target/debug/tekes-agent-ui-demo --offscreen /tmp/agentkit-ui-demo.png
python3 scripts/test-real-session-endpoints.py \
  --kernel-root /absolute/kernel-0.9.1-checkout \
  --pi-root /absolute/pi-1.3.0-runtime \
  --dsh-root /absolute/dsh-0.2.1-rc.2-runtime \
  --ui-demo "$PWD/ui/target/debug/tekes-agent-ui-demo" \
  --ui-output /tmp/agentkit-native-ui.png
```

`--native-selftest` 只使用本应用 fixture，通过 TekesUI 公开 selftest 接口投递本窗口事件和合成文本服务，
完成后自动关闭；不读取其他应用，不发送全局输入。不能与真实 endpoint 配置组合。
`--offscreen` 使用真实 GPU 渲染，不是原生窗口截图。三端集成脚本使用临时 HOME/dataRoot 和本地合成 provider，不调用付费模型；只传一个 runtime root 也可单端运行。
`--ui-output` 保留 Native 输出路径；Pi/DSH 分别加 `-pi`/`-dsh` 后缀。离屏 stdout 只输出数量/状态收据，不输出会话正文或凭证。

| 层 | 本轮证据 | 边界 |
|---|---|---|
| presentation / 组件 | 流式、取消、echo 去重、晚回包不清除其他草稿、scope 回调捕获、重连/分页 | 复用既有 controller |
| 第二 consumer | `consumer/` 独立包仅消费公开组件 API，编译与挂载测试 | 非第二原生产品 |
| 阅读 | 实际 Earlier 连续两页、尾追加/宽度/文字缩放、选区/焦点与 A/B 阅读恢复通过 | 原生窗口分页体验待验收 |
| 原生/物理/VoiceOver | 接收037的分层证据，公开pin188输入字节一致；本会话未重新现场操作 | 保留原二进制/源身份，不把GPU或合成输入当人声验收 |
| 真实 Native / Pi / DSH → GPU | 显式 SEP、RunController、已恢复 final assistant、离屏 PNG，见[三端记录](../docs/tekesui-three-endpoint-demo-20261019.md) | 非原生交互/真实模型 |
| 性能与完整UI成熟度 | 本轮不测性能，不宣布全组件/全主题验收 | UI仍experimental |

当前已接入公共异步唤醒、前插锚点、多行视口、输入元数据、受控事务和类型化动作，并固定公开pin。原始需求与历史收据保留，最新关闭范围、证据和上游Gallery CI失败边界以正式pin复验记录为准。
上述 RunSurface 早期试点不承担正式业务组件。当前 Composer、任务/子代理/SideChat、Summary 和附件状态的组件进度以本页开头及交付账本为准；完整 Markdown/工具内容与正式 Transcript 仍待上游能力。

本次 P0 联调与验收边界见 [记录](../docs/tekesui-p0-closeout-20261018.md)。

#19 指针迁移、共享选择修复与最新显式快照见 [记录](../docs/tekesui-pointer-metadata-20261018.md)；原生AgentKit验收仍未完成。

交接后的 [消费侧独立复验](../docs/tekesui-consumer-verification-20261018.md)：当前快照 UI27/headless76、真实 Native SEP 到 GPU 通过；尚非原生窗口或生产验收。

交付包的一键复现、失败收据和公开不可变 pin 字节比对见 [消费侧验收工具](../docs/tekesui-acceptance-tools.md)；工具不提升当前冻结候选。

Sidebar 重命名、分叉和归档通过共享 dispatcher 接入，归档后显式清除选择且保留草稿。`--components --thread-actions-selftest` 使用隔离 fixture 验证这些操作；不可与真实 endpoint 配置组合。见 [操作接入记录](../docs/rust-agent-ui-thread-actions-20261019.md) 和 [公共绑定说明](bindings/README.md#shared-inventory-and-thread-actions)。

`--components --model-choices-selftest` 使用隔离 fixture 验证模型目录、选择、发送前配置回执与流式结束。真实三端验证脚本可加 `--ui-model-choices`（须同时提供 `--ui-components --ui-demo`），仅在脚本自建的隔离会话与本地合成 provider 上执行额外一条模型选择证明消息。不要将离屏证明等同于三端原生窗口验收。

初始预设通过 `InitialPresetChoices` 适配 Native/Pi 初始目录和 DSH roster，仅在创建时应用。已有会话不提供预设切换。三端脚本的 `--ui-initial-presets` 在隔离状态下验证公开面板选择、共享 controller 创建和单次发送；离屏证明不替代原生预设验收；交互式新会话入口的进展另见 [新会话记录](../docs/rust-agent-ui-creation-20261020.md)。见 [预设接入记录](../docs/rust-agent-ui-initial-presets-20261020.md)。

`--components --creation-selftest` 使用隔离 fixture 验证交互式新会话、公开预设选择、组字屏障和正常 Composer 发送，保留原草稿。真实 endpoint 窗口需显式提供 `--create-cwd ABSOLUTE_PATH` 才启用新会话入口。创建失败保留 session/RPC 身份并阻止替代点击；专用失败对账 UI 尚未实现。见 [新会话接入记录](../docs/rust-agent-ui-creation-20261020.md)。

`--components --annotation-selftest` 验证已有批注评论的原生编辑、组字屏障和共享发送。真实三端脚本可加 `--ui-annotation-editor`，自动包含 context 证明；它不提供来源高亮或结构化历史卡片。公共 AnnotationEditor、原 binding 写入与快照确认流程见 [评论编辑接入](../docs/rust-agent-ui-annotation-editor-20261020.md)。

## 文件附件接入与三端验收

使用 `--components --file-attachments` 可打开本地合成 SEP fixture 上的真实 macOS 系统选择器；真实 endpoint 配置只按公开上传/policy 能力提供入口。宿主负责路径/权限/受限读取，Kit 负责原 destination、一次上传尝试和 canonical draft CAS。

三端脚本追加 `--ui-components --ui-attachments` 会从公开 Composer 入口捕获原请求，再以隔离测试文件 provider 执行上传、A/B 草稿隔离、共享 Send、历史恢复和重连检查。Native/Pi 另外从公开接口回读字节并拒绝其他 session；Pi 的历史文件语义是路径引用；DSH 保留文件元数据，通用文件下载保持不支持。该 flag 不启动系统对话框，也不等同于三端原生窗口验收。

[真实系统选择器记录](../docs/rust-agent-ui-upload-host-20261020.md) 与 [三端数据闭环记录](../docs/rust-agent-ui-upload-three-20261020.md) 分开保存；物理 IME、正式 Transcript 和阅读锚点仍未验收。

SidePanel 附件共享相同选择/上传/状态组件，按原 binding/request 保留；关闭来源后的回执不会进入父草稿。同 session 地址不同 controller 的操作也不会合并。有界子历史与独立 one-shot 子代理不提供上传。原生选择器与源替换测试分层证据见 [SidePanel 附件记录](../docs/rust-agent-ui-pane-uploads-20261020.md)。

正式 Transcript 前的公共 `TranscriptDocument` 只读组织消息、工具及 footnote lanes，保留已有身份/覆盖事实，第二 consumer 直接使用。它不含渲染或阅读控制器，不改变 Transcript/RunView 尚未交付的状态。见 [数据模型与证据](../docs/rust-agent-ui-transcript-document-20261020.md)。

`--components --session-notice-fixture --offscreen <PNG>` 只验证合法无 turn 提示进入文档出口，`documentProjection.sessionNotices` 必须为 1；不是正式提示卡片的视觉验收。

公共 `TranscriptSourcePosition` 和 `TranscriptSection::event_order()` 提供已知事件/容器顺序及明确的 unplaced 覆盖状态；ordinal 不作身份或锚点，正式文本块与工具交错仍待实现。见 [来源顺序说明](../docs/rust-agent-ui-transcript-order-20261020.md)。

`TranscriptItem.content_blocks` 规范化公开消息内部的 Text/Reasoning/ToolCall/Attachment/Unknown；None 明确保留聚合/旧内容覆盖状态。第二 consumer 直接读取覆盖与待解析工具引用，见 [内容块契约](../docs/rust-agent-ui-transcript-content-20261020.md)。

`--components --history-page-proof --offscreen <PNG>` 从公开 TranscriptPaging 按钮捕获原 A 的读取请求，验证 B 草稿隔离、源回执发布和新 epoch 历史保留，不是正式 Transcript 或阅读锚点验收。真实三端脚本加 `--ui-components --ui-history-pages` 可在隔离长会话上复验。公共接口见 [分页读取契约](../docs/rust-agent-ui-history-paging-20261020.md) 和 [分页控件](../docs/rust-agent-ui-paging-controls-20261020.md)。

`InputMessage` 是可独立挂载的用户消息行：默认 TekesUI Surface 气泡右对齐，caption 在气泡外靠右，正文使用公开只读 Content 文本服务。`--components --input-message-proof --offscreen /tmp/input-row.png` 在测试宿主挂载一个真实 snapshot 输入行；它不是完整 Transcript，附件/结构化原始信息现已支持，富内容预览与混合锚点仍待实现。

`OutputMessage` 从同一份 `TranscriptDocument` / `TranscriptSection` 消费机器内容，所有正文、推理、工具和原始数据详情靠左。`with_labels` 提供宿主 copy/工具详情格式化，样式继续由 TekesUI 提供。正文历史与流式更新使用同一公开只读文本组件，工具 DTO 仅在当前 section 查找；不猜测流/持久消息关联，不把工具结束当成 turn 结算。局部展开动作由宿主将 `drain_actions()` 的结果交给原行的 `handle_actions()` 后重新渲染。

`--components --input-message-proof --output-message-proof --offscreen /tmp/message-rows.png` 可挂载两类真实 fixture 内容行；真实三端脚本的 `--ui-components --ui-history-pages --ui-message-rows` 验证恢复历史的消息行消费、分页与重连。完整 Transcript、turn footnote、Markdown 富内容、混合锚点和物理 IME 仍有独立验收门禁，详见 [机器内容行](../docs/rust-agent-ui-output-message-20261020.md)。

`TurnFootnote` 独立组合 turn 的次级信息及共享动作，保持在机器正文外靠左。`TurnActionDispatcher` / `TurnActionRequest` 绑定原 RunBinding 与稳定 RPC ID，已知拒绝/完成可由下一次显式点击继续，未知结果保留禁用。新建分叉的导航始终由宿主核对原导航 token 后决定。

`--components --input-message-proof --output-message-proof --turn-footnote-proof` 显示独立内容行和 turn aside；`--turn-fork-proof --offscreen /tmp/footer.png` 明确启用一次分叉验证。真实端点脚本的 `--ui-turn-footnote` 只在隔离临时会话中验证同一动作链；这些 opt-in 探针不是正式 Transcript / RunView。接入、限制和分层证据见 [turn footnote](../docs/rust-agent-ui-turn-footnote-20261020.md)。


`TranscriptContent` 是公开的混合内容组件，组合规范化 InputMessage / OutputMessage / ToolActivity / TurnFootnote。用户附件位于正文气泡上方，外置 caption 在下方，整体右对齐；机器内容左对齐。样式直接继承默认 TekesUI 组件、tokens 与宿主主题，升级固定 UI 版本后统一体现，不维护 AgentKit 样式副本。

用户文本的外置复制按钮通过 `TranscriptCopyIntent` 捕获原消息文本，RunView 返回 `CopyTranscript`，只读组件单独提供 `take_copy_intents`。宿主按事件顺序、一次 `claim()` 后写系统剪贴板；离线和只读不会禁用已有文本复制。纯附件不提供文本复制。接入、状态边界与本轮失败/成功证据见[输入复制记录](../docs/rust-agent-ui-input-copy-20261020.md)。

输出 footnote 的复制也使用相同回调，由共享 `TranscriptSection::final_output()` 选择唯一最终正文；不复制推理或工具详情，不回退到最后一条消息。独立 TurnFootnote 消费方用 `update_with_output` 显式传入该目标；机器内容和操作继续靠左。公开 API、三端恢复验证与限制见[输出复制记录](../docs/rust-agent-ui-output-copy-20261020.md)。

```sh
ui/target/debug/tekes-agent-ui-demo --components --transcript-content
ui/target/debug/tekes-agent-ui-demo --components --transcript-content --offscreen /tmp/transcript-tree.png
```

实验宿主使用公共 ScrollContent，不提供稳定子项阅读事务或富文本能力；此内容树不等于完整 Transcript / RunView。真实三端脚本可加 `--ui-components --ui-transcript-content --ui-history-pages --ui-attachments`；不能同时启用独立的 `--ui-message-rows` / `--ui-turn-footnote` 探针。公开接入与证据见[内容树记录](../docs/rust-agent-ui-transcript-tree-20261020.md)。


[`RunView`](../docs/rust-agent-ui-run-view-20261020.md) 现提供独立的公开组合和类型化意图出口；`standard(namespace, row_pool, scope)` 直接包含混合内容树。实验宿主 `--components --transcript-content` 已复用它，窗口/导航/执行器仍由宿主提供。完整更新使用同帧 RunSnapshot、gate 和原绑定分页事实；焦点改变前按 `allows_focus_target` 维护组字屏障。原生 receipt 的 `runViewComposition.mounted=true` 只证明组合挂载，`completeRunView=false` 继续表示完整验收未通过。


`ReadOnlyTranscript` 为 Native 有界子历史复用同一内容树，显示真实消息与来源调用详情，只有显式读取回调。新 `PaneSnapshot::ReadOnlyChild` 带原读取调用 publication/epoch；`ChildPaneBinding::read_intent` 在 IO 前校验来源并只允许一次 admission。真实脚本可加 `--ui-components --ui-bounded-child --kernel-root <root>`，另存 `-bounded.png` 和 `NATIVE_UI_BOUNDED_COMPONENTS_RECEIPT`，不要求该 tab 同时挂 Summary。见[只读子历史契约](../docs/rust-agent-ui-readonly-child-20261020.md)。

共享绑定现支持原 committed feedback frame 的一次性评分/备注 CAS；三端真实验证与错误语义见[写入契约](../docs/rust-agent-ui-feedback-cas-20261020.md)。评价控件和备注编辑器尚待实现；`--feedback-write-proof` 仅用于显式启用的隔离验证。
