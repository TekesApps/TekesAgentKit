# #17 / #23：真实 Native 原只读子面板与三端同程序回归

按[契约](rust-agent-ui-native-child-pane-source-contract-20261024.md)，从 `79727f4` 继续，2026-10-25 完成验证。只修改 Rust AgentKit 默认关闭的验收驱动和离屏报告；不修改 Kernel、TekesUI、ClientKit、锁文件或精确 pin。

原 FamilyCatalog → OpenChild → ChildOpened 打开 Native SameSession 只读 pane。原 binding 的 `snapshot_with_bounded_source_witness` 返回同一次本地读取产生的 PaneSnapshot 与 typed head；该快照交原 HostResult::ChildRead admission，再核对实际缓存 source Arc/快照及原 ReadOnlyTranscript scope/内容行。没有替代 controller，没有 child Composer/发送，也没有父历史 fallback。

真实隔离 Kernel 0.9.3（已发布 revision `09508a9083c24c7accefc99516739f279b85913e`）和合成 loopback provider 验证了种子客户端退出后 Task 继续运行；通过公开 catalog 取得 bounded pointer。只匹配子 user 请求的 gate 恰好进入一次，原 mounted running 来源通过 Python 校验后才释放 HTTP400。原 sourceCall.phase running→error、runOpen true→false，head throughSequence 1→4，epoch 保持2，原 pane publication 3→5。sourceCall.callId/name/arguments、原 entries 前缀和合成 Task turn 身份保持；实际子 turn 的 response/error、turn/end 与投影 error item/status一致。未要求不存在的 sourceCall.error 字段。

新鲜只读审查发现“任意历史错误也可能充数”的校验风险，现严格关联唯一原子 turn/start、同 turn 的原错误/终止/投影，且失败原事件必须晚于 running cut；新增拒绝用例。父工具 arguments 中的 marker 不触发子 gate，真实 HTTP 路由测试验证父请求正常返回、仅子请求被持有并显式释放。

同一个最终 demo SHA `2ff35842d335c80bbc15b85616230e7bf159e9afabbb067bf698a570794cc515` 也通过既有 Pi1.3.0/DSH0.2.1-rc.2 原子 pane 闭环：Pi revision24→33，DSH29→54，均epoch2，各有一次验证后释放的失败 gate。Native共3次合成provider请求，Pi/DSH共16次；父中文草稿/选择保留，原关闭控件使旧来源退休并拒绝见证。复用精确 SHA `6aad53eb9910cb190ed5e887d27b1dd28169fea3245c1ed8f5eb27cb46a1215a` 的旧 headless **仅作公开种子/目录 helper**，没有用旧程序代替新 demo 的来源投影。

## 验证与首次失败

| 层级 | 实际结果 |
| --- | --- |
| Python | 262通过，新增7项 |
| 默认 UI / bindings+demo feature 全量 | 757/0/1；652/0/0，离屏报告修复前的完整回归 |
| 离屏报告修复后 RunPort 定向套件 | 默认19/0、feature19/0，含新增1项真实 controller 前进回归 |
| 格式 / 严格 Clippy | 默认与feature通过，末次报告修复后再次检查 |
| 普通 / feature demo | 末次源码重新构建，真实三端使用同一feature程序 |
| 独立公开 API consumer | 默认与feature执行通过；组件/presentation依赖未再修改 |
| 边界检查 | UI/headless静态检查通过 |
| 真实协议 / CPU mount | Native/Pi/DSH均通过，非桌面验收 |

首次预检使用错误的 RPC 地址字段而被拒绝，改为公开 parentSessionId 后通过，原失败保留。首次真实 Native 运行虽然两条来源帧校验通过、内部 host receipt 已写入 accepted=true，但最终进程退出失败，因此**该整轮不接受**：GPU 输出期间父 live binding 前进，离屏报告取更新后的快照核对旧树，coverage严格断言触发。修复为使用原 RunPort::displayed_snapshot，严格断言保留；定向回归证明旧树接受 displayed 原来源、拒绝更新后的 live 来源，原控件更新后才接受新来源。最终新程序三端进程退出0、来源/host/gate全套校验通过。证据中的中途 receipt 不能覆盖外层失败结论。

最终7个源码/测试哈希、首次候选、失败运行、原程序、固定输入、原帧、日志/收据和末次程序封存在 `.runtime-fixtures/native-child-pane-source-20261024/`；[JSON](rust-agent-ui-native-child-pane-source-results-20261024.json)可核对。owned Kernel/DSH均已退出，不影响既有Kernel进程。Cargo串行复用SSD shared-target，不新建target，删除0字节；固定快照、验收程序/证据和当前仍需缓存保留。

`79727f4` 的 push40108315461/PR40108318581 终态证据已回填上一切片：checks、opt-in、独立consumer通过，唯一失败仍#53；137/243来源闭包与PR merge父关系均已核实。新提交CI另核对。

## 仍未证明

Native仅证明head-only；累计分页 provenance、原pane重连/完整恢复与阅读位置、完整状态/tab/window组合仍未完成。这些PNG都是关闭子pane后最终父视图的离屏输出，不能冒称子pane像素/完整窗口或视觉验收。桌面、物理IME、OS AX、ProcessContainer、完整ClientKit与性能未验收；#17/#23保持open，#53门禁不豁免，不合并/tag/发布。
