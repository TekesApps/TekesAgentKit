# RunPort 命名与 ProcessContainer 接入边界

正式 Rust 业务组件名为 **RunPort**，组合 Composer、Transcript 和 Indicators；保留原 presentation、更新计划、事件捕获和阅读事务。ClientKit 对照源仍叫 RunView，该名称属于外部参考，不需要修改 ClientKit。RunSurface / ComponentDemo 继续只是验证宿主。

## API 迁移

UI 包仍为 `0.0.0 / publish=false`。本次直接统一未发布 API，不保留两套组件或别名。

| 旧 Rust 名称 | 当前名称 |
|---|---|
| RunView | RunPort |
| RunViewIds | RunPortIds |
| RunViewGeometry | RunPortGeometry |
| RunViewIntent | RunPortIntent |
| RunViewUpdate | RunPortUpdate |
| consumer::new_agent_run_view | consumer::new_agent_run_port |
| consumer::sync_agent_run_view | consumer::sync_agent_run_port |

`RunPort::standard(namespace, rows, scope)` 和原 `prepare_update` / `apply_prepared_update`、`publish_run_reading_update`、navigation/feedback/clipboard 出口继续复用同一实现。`run_view.rs` 源文件及对应测试改名为 `run_port.rs`。宿主应从公共 crate 根导入 RunPort，不依赖私有模块路径。

当前 demo 和实时端点验证脚本的收据键同步为 `runPortComposition`、`completeRunPort`、`runPortRouting`。旧验收 JSON、日志、程序、固定快照与历史文档保留原名称和程序归属，不能用新键改写旧结果。当前接入入口、bindings README、第二 consumer 和探针源码使用新名称。

## 组件与宿主边界

Sidebar、RunPort、SidePanel 的目标宿主边界是 TekesUI **ProcessContainer**。AgentKit 只提供业务组件、状态输入与类型化事件；TekesUI 负责容器的进程、通用 Content 生命周期、渲染、输入、焦点、IME、AX 和阅读基础能力。宿主负责窗口、整体布局、导航及系统能力适配。ProcessComponent 是上游容器内部/服务公共契约的一部分，不作为消费方另造宿主边界的理由。

核对精确公开 `09b4bab44b6dfa65487250ea816b610b9df59095`：

- `module-runtime/src/container.rs::new_view` 明确尚不支持原生远端 keyboard/IME/AX；`new_v1` 还明确不支持 scroll。默认 pointer 不拦截，只有显式 opaque-primary opt-in，尚无透明远端事件 disposition。
- `app-protocol/src/component.rs::ViewCapabilities::V1` 中 keyboard、scroll、text_ime、accessibility 均 false。存在协商字段不等于实现交付。
- 上游 `docs/design/process-component-view-v2-20261021.md` 仍将完整 remote keyboard/scroll/IME/AX 和 AgentKit 集成列为剩余能力。
- 当前 AgentKit 实际8包依赖闭包不包含 module-runtime/app-protocol；本轮没有为未交付迁移扩大生产依赖或伪装容器适配。

该版本不能承载完整交互式 Agent UI。已向上游提交 [TekesUI #48](https://github.com/TekesApps/TekesUI/issues/48)，列明远端编辑、焦点与事件消费、滚动、AX、执行模式说明及独立原生 consumer 验收。等待满足契约的固定公开版本，再审查完整依赖输入并接入原 ProcessContainer；不复制 IPC/输入/焦点/IME/滚动引擎，也不以图片预览替换 Composer。

## 当前可运行路径与验收矩阵

| 路径 | 当前实现与证据 | 本阶段交付边界 |
|---|---|---|
| 同进程原 Content | Sidebar、RunPort、SidePanel 由原 ContentHost 挂载；公开 consumer、组件和原生 demo 验证 | 保留可运行路径；**尚未通过 ProcessContainer 同进程执行契约验收** |
| 跨进程 ProcessContainer | 固定 SDK 有实验容器源码；当前 AgentKit 尚未接入 | 键盘/IME/滚动/AX/事件消费受 #48 阻塞；不能计为已验证 |
| Native / Pi / DSH SEP | 现有端点协议与既有三端原生验收保留各自程序、版本及输入范围 | Harness IPC 不等于 UI 跨进程容器；改名后程序须另记新证据 |

后续固定版本接收至少验证：真实 child PID/session、三个容器同窗口组合、中文组字与辅助 action/reveal、跨容器焦点、隔离草稿、分页/追加锚点、重连不重复提交、hide/show/unmount/crash、旧 session/geometry/AX 回包拒绝。组件、第二公开 consumer、同进程原生、跨进程原生分别留证，不能用上游 GPU 或 headless 服务测试替代 AgentKit 实际接入。

## 本阶段验证

UI workspace 490项（含第二公开 consumer）、工具31项、严格 all-target Clippy、fmt、原生构建、边界与实际8包241输入校验通过。原 RunView 实现及整套对应测试经精确名称替换后与 RunPort 文件逐字节相同，没有借改名另写组件状态。当前接入文档同步名称，历史证据保持不变。

新程序为 `.runtime-fixtures/AgentKitRunPort.app`，SHA256 `23a5ca72deb80700f757de739086cdc8eb2f06e76076d93378363b1e94e8a7ab`：

- 同进程原生阅读 fixture：分页20→44、Start/Latest、前插与向上阅读锚点、A/B草稿/阅读、重连和原编辑器焦点/合成中文预编辑保持，无提交。
- 综合同进程原生模式正常退出，原 Send/Stream/Cancel/drafts/reconnect、task/child路径输出既有 `_OK` 标记。该模式不声明正式混合 Transcript 阅读通过。最初临时包装脚本错误要求 JSON `RECEIPT` 而报错；已按模式核对 `_OK` 标记，保留观察记录，未修改程序断言。
- Pi1.3.0、DSH0.2.1-rc.2、Native Kernel0.9.3：真实协议、28回合历史种子/分页回传、RunPort 混合组件到 GPU 的 offscreen 路径通过，三端新收据均使用 `runPortComposition`。provider为隔离本地受控服务。Native 显式 UI-only 不覆盖完整生命周期/archive。
- 当前程序的真实 Pi 原生阅读尝试在窗口失去激活后停于6/16阶段；后续 DSH/Native 原生模式未执行。系统拼音两次尝试均在组字后失焦，当前程序没有通过收据。保留严格原断言和全部失败日志，不归因于尚未证实的外部原因，也不计为通过。

前一程序 `2e328704…` 的7项 Summary、原生阅读/Sidebar及系统拼音证据见 [Summary 层级记录](rust-agent-ui-summary-indent-20261022.md)与[公开 reveal 复核](rust-agent-ui-published-reveal-recheck-20261022.md)；它们不能迁移为当前程序新验收。更早真实三端阅读/反馈同样保留其原程序范围。当前 UI 的实体键盘、候选窗、VoiceOver、完整跨进程和配对布局均尚未验收。

机器可读范围及哈希见[同名 JSON](rust-agent-ui-run-port-20261022.json)，日志和验收程序保留在 `.runtime-fixtures/run-port-20261022/` 及对应 app 目录。

本阶段不宣布全量 UI 完成，不合并或打稳定 tag；PR #10 保持实验草稿。构建复用现有 `/tmp/agentkit-layout-text-292-target`，固定依赖不变，保留 WIP 和历史验收资料。
