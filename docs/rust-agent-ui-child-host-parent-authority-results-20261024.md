# #23：实际宿主 child factory 捕获父权限

按[契约](rust-agent-ui-child-host-parent-authority-contract-20261024.md)，在基线 `3b2c89e` 修正原 OpenChild dispatcher。只消费 TekesUI，pin 未变；只读 ClientKit 的挂载 lifetime/迟到 fencing 与 Tekes 的共享 pane adapters，未修改兄弟仓库。

## 实现

Summary 与 FamilyCatalog 的实际 OpenChild 命令现在同步携带原父 `Arc<RunBinding>`；异步阶段不按 scope 替换父对象。两入口与实际 dispatcher 使用同一个公开 host helper，从原父创建来源，继承祖先权限并在 await 前后复查。

bindings 的 `for_run` 默认50保持兼容，新增显式页大小入口及捕获父 Arc 的宿主入口。缓存与安装对宿主来源核对父对象身份；相同 scope 的替换父不能复用旧权限。旧 `for_run(&RunBinding)` 来源保留原兼容行为，不能据此冒称宿主已捕获它的对象身份。A→B→A 普通选择保留有效缓存，真正退休父 pane 则撤销后代的 Send/witness 权限。

主会话拒绝了候选中“优先猜相同 scope 的已选 Task”行为：当前实际入口来自主 Run，应捕获原主 Run producer；未来嵌套入口必须明确携带其 binding。新同 scope Task 回归对该候选先红，再以最终实现通过。没有增加嵌套 UI 入口。

## 验证

| 层级 | 实际结果 |
| --- | --- |
| 定向 child/panel feature | 27 通过 |
| 默认 UI 工作区 | 757 通过，0 失败，1 忽略 |
| feature bindings/demo | 648 通过 |
| 严格 Clippy / 格式 | 默认与 feature 通过 |
| 独立公开 consumer | 默认/feature 执行通过，检查新增公开方法类型；不冒称真实协议调用 |
| 当前普通/feature demo | 构建通过 |
| 边界 / 验收矩阵 | 通过，未豁免旧失败 |

定向回归覆盖：父对象身份、同 scope 替换拒绝、A→B→A、祖先退休后 Missing/Send/witness 拒绝、原读取 pending 时退休后拒绝返回、无效页大小，以及 SameSession 页大小7对应的原 journal 位置和下一页扩展，无 Composer/Send。嵌套祖先由合成来源通过**实际共用 host helper**验证，不能称为真实嵌套 UI 或真实 Native 嵌套协议验收。

本轮精确 feature demo SHA256 `dad457305f58851004e283f1707936026ffbfc132594047d154bb4a7747712e2`；复用 headless SHA256 `6aad53eb9910cb190ed5e887d27b1dd28169fea3245c1ed8f5eb27cb46a1215a`。真实 Pi1.3.0/DSH0.2.1-rc.2 重新经原 FamilyCatalog/OpenChild/HostResult/Composer/close 闭环通过，seed 命令各16/13，总计16次 owned合成请求；同epoch2原 running→error revision分别22→30、36→71。父非空草稿/选择保留，关闭后旧来源拒绝，两个 demo 与 owned DSH 均退出0。无付费模型、用户凭据或私有历史捷径。

证据位于 `.runtime-fixtures/child-host-parent-authority-20261024/`：基线源码tar、最终7文件覆盖哈希、被拒候选与红日志、最终测试日志/程序、公开consumer源码/锁、真实原帧/门与程序/退出收据保留，见[JSON](rust-agent-ui-child-host-parent-authority-results-20261024.json)。全部 Cargo 串行复用 SSD shared-target，没有新 target，没有删除缓存，本轮释放 **0 字节**；仍需的默认/feature缓存、程序与证据保留。

前一 `3b2c89e` 的 push `40103591762`、PR `40103596776` 已终态：checks、默认UI、新增bindings/demo opt-in与独立consumer通过；唯一失败仍#53严格嵌套阅读，后续CI demo build跳过。首次下载TLS失败保留，重试后两份原日志/artifact均取回，137文件不可变来源闭包、243文件SDK闭包和PR merge父关系核对通过。该CI只属于前一提交，新head另核对。

## 未完成

Native有界来源见证、实际子pane恢复/分页来源、动态Task pointer、完整窗口/物理IME/OS AX/ProcessContainer与完整ClientKit对齐仍开放。另只读核对Kernel **v0.9.3 tag `09508a9`** 发现 `sourceCall.phase` 明确为 running/done/stopped/error，而当前 AgentKit 将非running合成turn全标completed，列为下一独立小修；不改Kernel，不把该问题计作已修。#23不关闭，#53不豁免，不合并/打tag/发布。

20b4c44 CI终态补录：push40104896916、PR40104900090 的checks、新bindings/demo opt-in及独立consumer全部通过，唯一失败仍#53，CI demo跳过。两份原日志/artifact、137/243闭包及PR merge父关系已核对；不代替后续Native终态或head见证CI。
