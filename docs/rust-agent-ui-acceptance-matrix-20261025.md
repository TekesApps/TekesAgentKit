# Rust Agent UI：固定功能基线的分层验收

归属 #27/#28，关联 #15/#16/#17/#22/#23、[PR #10](https://github.com/TekesApps/TekesAgentKit/pull/10)。按[来源刷新契约](rust-agent-ui-evidence-refresh-contract-20261025.md)更新；[20261024 矩阵](rust-agent-ui-acceptance-matrix-20261024.json)作为不可变历史快照保留。

当前[机器矩阵](rust-agent-ui-acceptance-matrix-20261025.json)明确固定功能源码 `91129d701fbd2721cf3415b86d227900715ec353`。后续纯审计/文档提交不会自动改变功能证据基线；未来生产代码变化需要重新验证，不自动继承本次通过。19需求 × Native/Pi/DSH × 同/跨进程 × 9验证层级，共1026格。

当前计数：passed39（36共享组件测试格、3本次真实协议恢复格）、failed3、blocked342、not_run413、unsupported229；23个证据项、33条独立历史观察。矩阵检查通过仅证明记录及范围自洽，不把未验收格升级为通过。

`91129d7` 的 [push CI40136150914](https://github.com/TekesApps/TekesAgentKit/actions/runs/40136150914)与 [PR CI40136154649](https://github.com/TekesApps/TekesAgentKit/actions/runs/40136154649)均终态：checks、默认UI、opt-in子pane见证和独立consumer通过，唯一失败为TekesUI #53严格阅读，后续普通demo构建按原门禁跳过。两份完整日志/artifact、PR merge父关系、138个consumer源码输入和243个SDK文件闭包已核对；consumer源码摘要为 `db942475e6f74c2e61d86b84840d2b4987e93f5a64f15d47221d783d62758186`。PR下载首次及重试发生TLS EOF；保留失败记录后使用证书校验开启的TLS1.2取回并核验artifact digest，未重跑或改变CI结论。

新增 transport recovery、原图片字节、mixed附件顺序、运行中queue/submission身份四项需求。新增 protocol_integration 层，真实SEP协议与组件测试、独立consumer、离屏、窗口、IME、AX及ProcessContainer分别记录。共享组件单测只表示共享代码覆盖；普通consumer的构造/挂载不满足整个行为验收。

本次功能修复与验证见[owner最后入队复核](rust-agent-ui-submission-last-admission-20261025.md)和[精确结果JSON](rust-agent-ui-owner-admission-results-20261025.json)：受控fixture的owner退休竞态有红绿回归；另一项真实Native0.9.3/Pi1.3.0/DSH0.2.1-rc.2恢复回归使用本次新程序，三端close→Failed→原Reconnect→新库存/历史通过，9行和6份草稿保留，provider请求9→9。两项验证不可互相替代。

Pi mixed、Native/DSH idle mixed、DSH busy及Native busy失败均绑定各自原编译来源、归档程序和原始收据，保留为历史观察。Native busy失败继续追踪Kernel #86；不因为另两端通过或清理成功而提升。协议通过不代表桌面、图片位图UI或完整View parity。

来源绑定在 [evidence-bindings-20261025](evidence-bindings-20261025/)：原报告的baseline可能只是此前工作基线，绑定文件使用逐文件Git核实的实际sourceCommit；不改历史报告配合新的结论。每个protocol claim绑定单一harness/requirement/topology/layer与原assessment。CI-safe默认检查验证受控报告及链接SHA；本机模式额外核对原程序、原始收据、manifest及声明Git版本的源码字节。

```sh
python3 scripts/check-agent-ui-acceptance-matrix.py
python3 scripts/check-agent-ui-acceptance-matrix.py --verify-local
python3 -m unittest scripts.tests.test_agent_ui_acceptance_matrix -v
```

以上默认与完整本机校验均通过；最终矩阵定向26项通过，整套Python318项通过。原程序、收据和Git源码都已复核；不是新的桌面、IME或容器执行。

桌面、系统中文组字、VoiceOver和完整交互式ProcessContainer仍未验收。TekesUI #53严格阅读失败保留，不skip、不放宽。PR保持draft，完整Issue不关闭、不合并或打完成tag。本轮复用SSD共享target，验收程序/收据保留；新增target为0，删除缓存为0字节。
