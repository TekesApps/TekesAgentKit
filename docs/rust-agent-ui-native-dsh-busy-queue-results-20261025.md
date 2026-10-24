# Native / DSH 运行中队列与原提交确认（2026-10-25）

归属 #16/#27，关联 #15/#17/#28，PR #10。此前 Native/DSH mixed 证明的是空闲时 Queue 模式；本轮独立制造流式执行中的 pending input，不将两者混为一项验收。

DSH 0.2.1-rc.2 的公开 `inbox` 原投影现由 adapter 转换为 `agentkit.queue.v1`，完整保留 message、source、item ID、原序号与两组内部顺序。SessionEndpoint 的默认关闭 hook 显式声明本地适配器权限，HostedEndpoint 透传；通用 projector 只处理授权 key 的版本和物化，不解释任意 inbox。journal、control、inventory 共享同一版本门禁；旧序号不覆盖、同序号冲突拒绝、明确双空才清空，迟到 baseline 不清除更新队列。同序号冲突规则仅限授权队列，其他 opaque 投影保留既有接受规则。切换适配器时清除旧权限值，避免将保留的 opaque 数据提升为队列权威；同一适配器重连保持持久序号。详见[契约](rust-agent-ui-dsh-queue-projection-contract-20261025.md)。

Composer 原队列回显确认补读公开 `message.source.rpcId`，且必须为 `role=user` / `source.kind=user`，仍按原 scope、目的地与提交 ID 隔离。Native DTO 的回归先红后绿，覆盖跨会话、错误角色/来源、错误 RPC 和已确认后的迟到失败回执；不引入第二套提交状态机。

真实 busy provider 先输出不完整的 `ENDPOINT_` 并暂停。在收到原公开 inbox/control 见证、原 RunSnapshot.queue 和 SubmissionState 后，独立 runner 重验原 scope/RPC/item、附件原字节 SHA、提交已确认且无 provisional，才允许输出剩余 `REPLY_OK`。DSH 已通过该门禁、排队后实际派发、第二个空正文 mixed 会话、公开历史与重连。DSH 不提供通用文件下载；文件仅证明原上传、公开历史和 provider handle，图片另核对公开读取和 ImageIO 解码。

Native Kernel 0.9.3 仍有单独的未通过项（[Kernel #86](https://github.com/TekesApps/TekesKernel/issues/86)）：真实 hold 已有 committed user 和 run_open，排队 RPC 已 admitted，但原 SessionControl 只报告空 queue；另一条独立只读 control 订阅也观察到空 baseline / `queue.state.items:[]`。探针按门禁失败；只为退出 owned 进程释放 hold 的清理动作不算验收。未改 Kernel、未将 admitted 或后续回复冒充 busy queue 可见性。

历史失败、每轮源码和程序、原公开帧、provider 观测、退出收据在 SSD `.runtime-fixtures/native-dsh-busy-queue-20261025/` 分别保留，不覆盖旧证据。新提交与 CI 的精确身份在验证完成后另行绑定。桌面、物理 IME、OS AX、真实模型理解和完整 Views 仍未验收；TekesUI pin 不变，严格嵌套阅读 #53 与图片控件 #117 仍开放。未合并、未 tag、未关闭完整 Issue。


最终冻结程序 SHA256 `fc44d6dd875e2a7b4053b875fbec561c38ce4b0f21d3906b14901e0ffcbd0638`，三次实际运行各冻结557项输入，均与待提交工作树核对一致：`release-dsh` 真 busy 通过，`release-native` 按空 control 门禁失败，`release-idle` 两端原空闲 mixed 回归通过。两端空闲的4次业务请求与DSH1次标题请求分别核对，不把其作为Native busy成功。Native失败的probe受控退出-15、processGroupExited=true；两端owned harness退出0，releasedAfterQueueValidation=false。

DSH 排队原inbox序号17；消费后22明确双空，重连epoch2→5后27仍双空；原Run队列同步为空。原message.id成为后续committed user.recordId，原source/RPC、独立turn和附件不变。公开图片回读、provider原bytes与ImageIO仍通过；无付费模型/真实文件理解结论。

最终验证：Root401/0/0，UI781/0/1（1既有忽略），Python299/0；Root/UI all-targets严格Clippy、最终example严格Clippy/构建、fmt、headless/UI边界和验收矩阵通过。独立公共API consumer已实际运行exit0，138项来源SHA `b4d81185f6336aa401120abedc76f9ad03dd794dc3280da82302270821e1a5dc`；准备时HEAD仍为3bda2f6、消费的是明确WIP来源，提交后另绑定。精确SDK pin50ec49f、8包243文件及闭包SHA核对通过。严格reading仍exit1、accepted=false、detailVisibleAfterAppend=false，未豁免#53。

[结构化收据](rust-agent-ui-native-dsh-busy-queue-results-20261025.json)包含实际门禁、原公开帧与范围。所有Cargo复用SSD共享target，没有新target；本批缓存删除0字节，失败与最终证据/程序均保留。PR精确新head CI后续在Issue回填，不能把先前3bda2f6的CI移给本批。
