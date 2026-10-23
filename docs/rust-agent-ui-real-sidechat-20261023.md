# 真实 SideChat 组件接入（离屏）

对照 ClientKit `c289bc9` 的 `ClientSideChatRunView` 及 Tekes 的 `SidePanelHostView`：
SideChat 内容来自临时 fork 自己的 Run，不能借父会话内容或创建一个无来源的新 draft；
只有成功释放 fork 才关闭对应 pane。参考仓库、Kernel 和 TekesUI 均未修改。

新增独立 `--ui-sidechat` 验收入口。使用原 `SideChatBinding`/controller、SidePanel、RunPort、
Composer、TranscriptContent 和公开事件回调，未新增业务状态机、平台输入桥或进程容器。
该 profile 明确拒绝与 native、其他独立 UI profile 混用。

| Runtime | 本轮结果 |
| --- | --- |
| Native / Kernel 0.9.3 | 原按钮创建独立临时 fork；原 pane Send；本轮输入对应的 completed turn/final reply；父历史隔离；Summary 切换后的原 Composer IDs/草稿；新 epoch 历史无重复；原 Close 确认 discard 后移除 pointer/控件，旧 authority 失效；父与另一会话草稿不变。 |
| Pi 1.4.0 | 同上；实际 Pi 临时 session 文件及 native pointer 走原 SEP，未使用本地协议 fixture 代替。 |
| DSH 0.2.1-rc.2 | 公开 runtime 没有 `session/discard`，原 SEP 不提供 ephemeral fork；只验证原 create 不可用、无 fork 命令/指针、两份原草稿保留。`scratchLifecycleAccepted:false`，不能称为 SideChat 可用。 |

创建、发送、切换及关闭由原控件的 action 产生。新 fork 的首个 action 还会产生原自动反馈读取，
验收宿主正常执行该只读 callback，再核对业务命令；不吞掉读取，也不把它误算为第二次发送。
完成断言同时绑定本轮输入、turn ID、completed 状态及同 turn 的非 provisional final reply，
不能使用 fork 继承的父回复证明本轮成功。

DSH 的限制与 ClientKit `DSHHostEndpoint.supportsEphemeralFork` 的 discard 探测策略一致；
不以普通 fork、archive 或私有文件删除伪装 disposable fork。未来 runtime 公开此能力后应重新
固定版本并单独验收，不在消费侧猜开权限或放宽门禁。

最终程序和源码身份、三份原始收据见[JSON](rust-agent-ui-real-sidechat-20261023.json)。
`.runtime-fixtures/real-sidechat-ui-20261023/` 保留原始失败、成功日志、PNG、验收程序和源码副本。
早期探针的命令计数、DSH 能力假设和遗漏收据字段均曾失败；最终 v5 独立完整运行退出 0，
旧记录不回写为通过。

验证：6 项既有 SideChat binding 回归、74 项 Python、严格 demo all-target Clippy、fmt、
静态依赖边界通过。仅 demo/验证代码变化，未重复宣称此前全 UI 616/1 忽略或第二 consumer
在本轮重跑。构建复用共享 target，没有新 target。

本轮无桌面窗口、无截图权限请求、无物理 IME/分页锚点/跨进程验收；Native 显式 UI-only，
排除完整 Kernel 生命周期和子历史协议验收。最终整体 UI 交付仍未完成。
PR #10 的 abfc1c2 CI 原始 #53 失败收据保留，严格门槛不豁免。
