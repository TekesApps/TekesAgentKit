# #22 / #24：真实多端 SideChat 原分发与生命周期

[实施契约](rust-agent-ui-real-multi-sidechat-contract-20261024.md) · [机器收据](rust-agent-ui-real-multi-sidechat-results-20261024.json) · [PR #10](https://github.com/TekesApps/TekesAgentKit/pull/10)。本轮只改 AgentKit 验证工具/离屏入口；没有修改 TekesUI、Kernel、ClientKit 或 Tekes，也没有新业务状态机或正式组件类型。

## 已完成

原真实 SideChat profile 只消费首端兼容字段。新 opt-in `--multi-sidechat-delivery-receipt PATH` 要求显式三端配置和 offscreen；从实际 `view.bindings()` 验证 Native→Pi→DSH 顺序，每端两个原普通父 Run，Pi 保持第二端。不得把 Pi 改成首端或用旧单端 proof 冒充多端。

当前普通程序连接 Native 0.9.3、Pi 1.3.0、DSH 0.2.1-rc.2，使用隔离 home/data 与合成本地 provider，经原 Sidebar/SidePanel/Composer 控件、原 DemoCommand、Host dispatcher 及原 HostResult handler 完成：

- Native/Pi 原 Create、真实 fork、原 SidePanel RunPort/Transcript、原输入框 Commit/Send 与最终回复；独立 scratch 只使用所属 runtime，不能借首端 owner。
- 每端再发受控流，等待实际启用的 Cancel 控件和真实 CANCEL_PENDING，然后原 Cancel；终态均为 canonical `aborted`。不是撤销未派发 ticket，也不把 Idle 当成功。
- Summary 返回、原 tab 重开、原 Composer IDs/草稿/controller 保留；对应端 reconnect 后 epoch 2→5，正常输入与取消输入各恰好一次。
- Pi 的实际 Create 结果被截留后切到 Native，再交原 handler：只缓存原 Pi 结果，不抢主/侧栏选择或焦点。Pi 正常响应由本地 provider 短暂持有，先选 Native，再观测 Pi 回复在后台完成、交回原 Send 完成结果；选择/焦点不变。Pi 的确认 Close 结果另验证迟到不扰当前端。
- 原 Close 真实 discard 后，原 pointer 退休、binding access=Missing、pane 移除。六个父 Run 的草稿和历史保持隔离，正常及取消输入均未串入。
- DSH 原 Create 为 disabled，Press 无 fork 命令、无 scratch pointer；unsupported 单独记录。

合成 Preedit 只验证原组件组字期间不发送。缓存 tab 后通过 TekesUI 现有公开 Focus 事件模拟用户聚焦原编辑器，再输入；**不声称自动 tab 回焦、系统中文 IME、OS AX 或桌面验收**。原 pane 首次挂载的只读 feedback 请求仍走原 Host dispatcher，不能误当额外 Send，也没有被抑制。

## 检查与程序身份

11 项现有 SideChat binding/multi-host 定向回归通过；最终严格 UI all-targets Clippy、格式、边界和验收矩阵检查通过。Python 全套 177 通过，其中新 7 项拒绝测试检查错误 endpoint 顺序/来源、缺失取消/迟到证明、重复输入、旧 epoch、错误 unsupported 和误报桌面，以及 opt-in 取消窗口的参数边界。独立公开 consumer 实际执行六挂载；构造器/组件包闭包未变化。最终普通 demo 构建及三端离屏完整运行通过。新提交的全部 CI 另行核对，不能把此前 f399356 的 CI 当新提交结果。

最终程序 SHA：

- headless：`935ec6b61fc277927bff460b6a29d4074ae348f1215b97b8f759583879694d3a`。
- demo：`4be2543bf4aaf6adf1ebd5ac2c3ea92182bd546ec7de154dbd83d667c7be4ebf`。

SSD `.runtime-fixtures/real-multi-sidechat-20261024/verified6/` 保留命令、实际版本/程序身份、逐端输入输出、delivery/acceptance、离屏图与阶段日志；522 个来源文件在运行后逐一 SHA 复核不变，`source-verified6` 保留源码。最终 headless/demo 与 Cancel 控件竞态的失败程序另行归档；归档程序不再运行。

历史 attempt1–3 的驱动假设失败均保留：重复点击父行改变焦点；把正常只读 feedback 回调误当业务命令；缓存 tab 缺少显式用户 Focus。attempt4 只证明其原驱动版本；加强断言后的 `verified` 运行发现 snapshot 可取消但 UI Cancel 未挂载的等待竞态。最终同时检查原实际控件可用，且本 profile opt-in provider 持有 3 秒（旧 profile 默认行为不变），仍要求真正 `aborted`。这些是验证驱动修正，不称产品/SDK 修复，也不提升旧程序的验收身份。

## 仍开放

#22/#24/#27 整体不关闭：真实宿主 picker/附件、完整窗口/chrome、布局/物理 IME/OS AX 和 ProcessContainer 尚未完成。TekesUI #53 严格阅读门槛仍保留；不合并、打 tag 或发布。当前固定公开 TekesUI pin 仍为 `50ec49f3446924fdbc66b3cff22cd8cfe150fe7c`。

全部构建复用原 SSD shared-target，没有新增 target，没有干扰其他任务。本批未删除缓存，实际释放 0 字节；保留固定依赖、源码、WIP 与所需证据。
