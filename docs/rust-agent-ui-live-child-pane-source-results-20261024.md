# #17 / #23 / #27：实际子 pane 原来源验收

基线 `04481a7`，实现遵循[接入契约](rust-agent-ui-live-child-pane-source-contract-20261024.md)。只读参考 ClientKit `c289bc9`、Tekes `8858ccd`；TekesUI 精确 pin `50ec49f3446924fdbc66b3cff22cd8cfe150fe7c` 未变。结构化收据见[JSON](rust-agent-ui-live-child-pane-source-results-20261024.json)。

## 实现与实际协议结果

默认关闭的 demo feature 透传原 bindings 的 `acceptance-source-witness`。隔离 seed 进程完成后退出，demo 再连接，沿原 FamilyCatalog 控件→OpenChild/HostResult→缓存 ChildPaneBinding→原 Composer Commit/Send→原 SidePanel close 执行。没有将 child 打开成 primary Run，没有另建 controller 或复制 presentation。父 scope 的非空中文草稿与选择保留，关闭后旧 binding 的来源捕获被拒绝。

| 实际 harness | seed 公开命令 | epoch | 原 running→error revision | 结果 |
| --- | ---: | ---: | --- | --- |
| Pi 1.3.0 | 16 | 2 | 25→33 | accepted，进程退出 0 |
| DSH 0.2.1-rc.2 | 13 | 2 | 34→76 | accepted，进程退出 0 |

两端总计 16 次 owned loopback 合成 provider 请求。每端恰好一个 marker 请求被 hold；Python 接到独立原 running 帧并核对 parent.thread+child.session、Ready/epoch、原 User 来源位置及 opaque turn 后才释放 HTTP 400。随后从同一原 binding 捕获 failed 帧，核对同输入、同 turn、同 epoch、递增 revision 与自然结算。门均显式释放，无 abort/timeout。真实协议使用合成响应；不代表付费模型质量或服务端事务一致。

最终 demo SHA256：`e9c27f7970babfdc6cbbc3d1afb91fc5b893272ae81818a14d8ea97db279d43d`；复用 headless SHA256：`6aad53eb9910cb190ed5e887d27b1dd28169fea3245c1ed8f5eb27cb46a1215a`。程序、最终 9 文件输入哈希、原帧、门收据、commands、stdout/stderr、退出码与固定依赖保存在 `.runtime-fixtures/live-child-pane-source-20261024/`。`final/` 是最终精确源码程序的接受结果；`send-ready/` 为前一程序的成功，不混用程序身份。

初次 `verified/` 与 `catalog-ready/` 失败保留：来源就绪早于组件 publication，随后 Send 按钮也尚未就绪。验收现等待原控件发布并启用，再执行一次原回调；未修改生产状态机、直接发送或清空 dispatching。早期编译失败和候选代码同样归档。

## 验证层级

- Python：6 项定向、255 项全量通过。
- 默认 UI：752 通过、0 失败、1 忽略；feature bindings/demo：643 通过。
- 上述广泛回归在最后仅扩展 CLI 参数前缀拒绝之前执行。当前精确代码另重复默认 CLI 2 项、feature CLI 1 项、两配置严格 Clippy/构建和真实双端闭环。误写、等号形式或仅 receipt 参数均在宿主/窗口创建前拒绝。
- 默认工作区和 feature 严格 Clippy、格式、两配置独立公开 consumer 执行、headless/UI 边界及验收矩阵检查通过。root、bindings、components、锁和 SDK pin 输入不变。
- CI 增加 demo feature 验证。前一提交 `04481a7` 的 PR `40100645177`、push `40100642243` 已终态核对：consumer、原 opt-in 步骤及常规测试通过；唯一失败仍是 TekesUI #53 严格嵌套阅读。原 artifact、日志和 PR 合成 merge 父关系已归档，不能替代新 head CI。

全部是组件/真实协议与离屏语义操作。PNG 保存的是关闭子 pane 后的父视图，不是 child 布局验收；没有桌面窗口、截图、物理 IME 或 OS AX。Cargo 串行复用 SSD shared-target；本轮没有新 target、没有删除缓存，释放空间 **0 字节**。

## 剩余工作与新发现

实际 demo 的 OpenChild factory 仍按 scope 调用 `open_with_page_size`，没有捕获原 parent binding 的祖先 authority；现有 `for_run` 合成测试不证明该宿主路径。当前 UI 只提供一层子 pane，不能据静态工厂差异声称用户已能触发嵌套入口；下一切片在 #23 修正宿主构造并验证退休、迟到和 A→B→A 的不同语义。

Native 有界 source witness、子 pane 重连/分页来源、动态 Task pointer、完整窗口/布局/物理 IME/AX/ProcessContainer 和全量 ClientKit 对齐仍未完成。#17/#23/#27 保持 open；#53 门槛不豁免，不合并、不打 tag、不发布。

3b2c89e 的终态补录：push40103591762、PR40103596776 的checks、默认UI、bindings/demo opt-in和独立consumer均通过，唯一失败仍#53严格阅读。原日志/artifact已保存并核对137/243来源闭包及PR merge父关系；不能代替后续父权限修复CI。
