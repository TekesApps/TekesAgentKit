# Rust Run demo（开发试点）

只消费 TekesUI，复用 AgentKit v0.3.8 的 `SessionRuntime` / `RunController` / `RunSnapshot`。
组件层不实现另一套 Run 状态机。默认 headless workspace 和发布 API 保持独立。

[短接入契约](../docs/tekesui-integration-contract.md) 定义所有权、输入、回调、焦点、IME 和阅读责任；
[TekesUI 缺口](../docs/tekesui-gaps.md) 列出所需上游事件/属性，
[七项开发任务](../docs/tekesui-development-tasks.md) 给出优先级与逐项验收标准。

![基础 Run 离屏渲染，非原生窗口截图](docs/run-offscreen.png)

## 正式组件开发

`RunSurface` 仅是现有协议/控件测试宿主，不是标准 RunView。正式组件契约、App 的 input/output/footnote 布局、基础 Issue 和完整交付账本见 [Rust Agent UI 接入契约](../docs/rust-agent-ui-contract.md)。

当前已有可独立挂载的 `Composer`、`Indicators`、`Sidebar`、`SidePanel`、`SummaryView` 和 `ThreadRename` 首批实现。正式 Transcript、RunView，以及任务/子代理/SideChat 的 Run 内容、完整宿主集成和原生窗口验收仍在进行中。不要把下面的测试宿主截图当成最终 Transcript 设计。

模型/预设新增独立 `ComposerChoices` 内容和 `ComposerSettingsRequest` 绑定；公开目录输入、回调与宿主责任见 [接入说明](bindings/README.md#composer-model-and-preset-content)。模型已接入 demo 的公开 `session.models` 目录与发送前配置，预设等入口仍待接入；见 [模型闭环记录](../docs/rust-agent-ui-model-integration-20261019.md)。

新增实验 [runtime 绑定包](bindings/README.md)，连接组件回调、既有 RunController、历史 lease 与 SideChat 通知；不复制业务状态机。原生 demo 尚未迁移到正式 RunView。

`--components` 启用实际 Composer/Indicators + RunBinding 的独立原生验证窗口，可与 `--native-selftest`、`--offscreen` 或显式 `--endpoint-config` 组合。已完成合成文本服务、发送/取消、草稿隔离和重连的窗口验证；真实三端离屏接入可加 `--ui-components`。截图与验收范围见 [原生组件接入记录](../docs/rust-agent-ui-native-controls-20261019.md)。它不包含正式 Transcript，也不是标准 RunView。

最新窗口还接入了实际 Sidebar / SidePanel / SummaryView：共享列表和置顶、异步会话选择、当前父会话 Summary 已验证。共享服务的自定义 pin 回传/通知修正仍未发布；未接入的重命名/分叉/归档交互会在 RPC 前明确拒绝。运行方式与范围见 [Sidebar/Summary 记录](../docs/rust-agent-ui-sidebar-summary-20261019.md)。

SideChat 增量使用同一 runtime 的 `SideChatBinding`：已可创建 owned scratch、挂载独立 Composer/Indicators、发送与流式状态回传、取消后确认丢弃，并隔离父会话草稿。关闭结果不确定时保留 pointer/草稿并禁用发送；无自动重试或私有数据库读取。完整 Transcript 尚未接入，仍不是正式 SideChat RunView。公共接入与 reconciliation 责任见 [绑定说明](bindings/README.md#sidechat-tab-authority)。

任务 pane 使用 `TaskPaneBinding` 和既有 TaskPanelState：跟随当前任务行，保留同一 run 的草稿，指针变化时撤销旧控件及回调。demo 支持显式宿主任务源和已有 Summary OpenTask 动作；同会话 todo 保持原来的无独立 run 导航语义。任务原生脚本验证独立输入/发送/取消及父草稿隔离；正式任务 Transcript 仍待实现。详见 [任务绑定说明](bindings/README.md#task-pane-resolution)。

子代理增量使用 `ChildPaneBinding`：明确接收 parent 与历史布局，独立 session 挂载自己的 Composer/Indicators，有界历史只读且没有 Composer。Summary 的子代理动作已接入宿主布局注册；重复打开复用原实例。原生 fixture 验证发送/回传/取消和有界分页，正式子 Transcript 仍待实现；真实三端子 pane 原生交互尚未验收。见 [子代理接入记录](../docs/rust-agent-ui-child-20261019.md)。

## 依赖

- Rust 1.95.0、macOS + Metal（原生窗口/文本服务/GPU）；Python 3.11+（准备和边界检查）。
- AgentKit 基线 v0.3.8（公开 main `f1daa0ffcf6ced41e21f351e22845455ca78006c`）；Native 真实集成基线 Kernel v0.9.1。
- TekesUI composition 仍是实验 API，但源码现在固定为公开 Git commit `431093786de47b48cd566d0d9c0eafa088b10d68`；6 个直接依赖和2个传递依赖均由 Cargo 按该完整 rev 取得，ui/Cargo.lock 同步固定。
- `tekesui-source.json` 的 kind 为 `published-git-revision`，188文件 SHA-256 为 `03721866a362b1d31261ebbbc86140eb07e9e23320871c325234a5212591213d`，与此前已验收037候选完全一致。固定的是不可变commit，不跟随分支HEAD。
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
本轮不接任务、子代理、SideChat、Summary、Markdown、工具卡片或附件 UI。

本次 P0 联调与验收边界见 [记录](../docs/tekesui-p0-closeout-20261018.md)。

#19 指针迁移、共享选择修复与最新显式快照见 [记录](../docs/tekesui-pointer-metadata-20261018.md)；原生AgentKit验收仍未完成。

交接后的 [消费侧独立复验](../docs/tekesui-consumer-verification-20261018.md)：当前快照 UI27/headless76、真实 Native SEP 到 GPU 通过；尚非原生窗口或生产验收。

交付包的一键复现、失败收据和公开不可变 pin 字节比对见 [消费侧验收工具](../docs/tekesui-acceptance-tools.md)；工具不提升当前冻结候选。

Sidebar 重命名、分叉和归档通过共享 dispatcher 接入，归档后显式清除选择且保留草稿。`--components --thread-actions-selftest` 使用隔离 fixture 验证这些操作；不可与真实 endpoint 配置组合。见 [操作接入记录](../docs/rust-agent-ui-thread-actions-20261019.md) 和 [公共绑定说明](bindings/README.md#shared-inventory-and-thread-actions)。

`--components --model-choices-selftest` 使用隔离 fixture 验证模型目录、选择、发送前配置回执与流式结束。真实三端验证脚本可加 `--ui-model-choices`（须同时提供 `--ui-components --ui-demo`），仅在脚本自建的隔离会话与本地合成 provider 上执行额外一条模型选择证明消息。不要将离屏证明等同于三端原生窗口验收。
