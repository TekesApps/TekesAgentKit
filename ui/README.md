# Rust Run demo（开发试点）

只消费 TekesUI，复用 AgentKit v0.3.0 的 `SessionRuntime` / `RunController` / `RunSnapshot`。
组件层不实现另一套 Run 状态机。默认 headless workspace 和发布 API 保持独立。

[短接入契约](../docs/tekesui-integration-contract.md) 定义所有权、输入、回调、焦点、IME 和阅读责任；
[TekesUI 缺口](../docs/tekesui-gaps.md) 列出所需上游事件/属性。

![基础 Run 离屏渲染，非原生窗口截图](docs/run-offscreen.png)

## 依赖

- Rust 1.95.0、macOS + Metal（原生窗口/文本服务/GPU）；Python 3.11+（准备和边界检查）。
- AgentKit 基线 v0.3.0；Native 真实集成基线 Kernel v0.9.1。
- TekesUI composition 为实验 API：`tekesui-source.json` 固定 base commit 与逐文件 SHA-256。
  当前包含必要的未提交上游实现，**base commit 本身不能重现**；需要显式选择包含匹配文件的源码。
  无发布 pin 前不作为可发布组件。所有包 `0.0.0` / `publish=false`。
- 上游代码只读取并复制到忽略的 `.deps/tekesui`，不修改原工作树，不提交依赖源码。
  若出现 source drift，需协调取得原快照或正式上游 commit；不要用 `--capture` 绕过错误。

## 构建与运行

从 AgentKit 根目录：

```sh
python3 scripts/prepare-tekesui-demo.py --source /absolute/explicit-tekesui-source
python3 scripts/check-ui-boundaries.py
cargo build --manifest-path ui/Cargo.toml --locked -p tekes-agent-ui-demo
ui/target/debug/tekes-agent-ui-demo --frame-pump
```

默认 transport 是独立的本地 SEP fixture：两个会话、有界历史、逐块回传和可取消运行，不需要凭证或模型。
按钮提供发送、停止、历史分页、回到最新、重连、切换 A/B。Composer 仅单行，Enter 不发送。
`--frame-pump` 是显式开发驱动：上游缺少后台唤醒代理时持续请求帧，不能用于空闲性能或生产验收。

真实 Native endpoint 使用宿主显式 JSON；session 必须已存在且有宿主配置的模型/执行参数：

```json
{"endpointId":"native-ui","url":"http://127.0.0.1:PORT","bearerEnv":"MY_EXPLICIT_BEARER","sessions":["SESSION_A","SESSION_B"]}
```

```sh
ui/target/debug/tekes-agent-ui-demo --frame-pump --endpoint-config /absolute/native.json
```

令牌仅从 `bearerEnv` 指定的环境变量读取。发送会使用该 session 的实际模型配置。
本轮 UI host 只提供 Native 配置；Pi/DSH 独立 headless 集成继续保留，未宣称原生 UI 三端验收。

## 验证

```sh
cargo test --manifest-path ui/Cargo.toml --workspace --locked
cargo clippy --manifest-path ui/Cargo.toml --workspace --all-targets --locked -- -D warnings
ui/target/debug/tekes-agent-ui-demo --frame-pump --native-selftest
ui/target/debug/tekes-agent-ui-demo --offscreen /tmp/agentkit-ui-demo.png
python3 scripts/test-real-session-endpoints.py \
  --kernel-root /absolute/kernel-0.9.1-checkout \
  --ui-demo "$PWD/ui/target/debug/tekes-agent-ui-demo" \
  --ui-output /tmp/agentkit-native-ui.png
```

`--native-selftest` 只使用本应用 fixture，通过 TekesUI 公开 selftest 接口投递本窗口事件和合成文本服务，
完成后自动关闭；不读取其他应用，不发送全局输入。不能与真实 endpoint 配置组合。
`--offscreen` 使用真实 GPU 渲染，不是原生窗口截图。Kernel 集成脚本使用临时 HOME 和本地合成 provider，不调用付费模型。

| 层 | 本轮证据 | 边界 |
|---|---|---|
| presentation / 组件 | 流式、取消、echo 去重、晚回包不清除其他草稿、scope 回调捕获、重连/分页 | 复用既有 controller |
| 第二 consumer | `consumer/` 独立包仅消费公开组件 API，编译与挂载测试 | 非第二原生产品 |
| 阅读 | 实际 CoreText 布局；向上阅读后尾追加/宽度变化保持阅读意图，显式回到最新 | 不含头插锚点 |
| 原生合成文本服务 | 中文 preedit/确认、发送/停止、A/B 草稿、重连、历史数据 | 非物理输入法 |
| 真实 Native → GPU | 认证 SEP、RunController、历史回传、离屏 PNG | 非原生交互/真实模型 |
| 物理中文 IME、VoiceOver、性能 | 尚未验收 | 不可由合成事件或离屏渲染替代 |

生产阻塞项是异步唤醒和历史前插锚点；多行编辑、修饰键/多击选择、受控值事务等需求见缺口清单。
本轮不接任务、子代理、SideChat、Summary、Markdown、工具卡片或附件 UI。
