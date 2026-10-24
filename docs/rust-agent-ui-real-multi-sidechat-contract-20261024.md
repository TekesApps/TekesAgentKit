# #22 / #24：真实多端 SideChat 的下一阶段契约

关联 [#22](https://github.com/TekesApps/TekesAgentKit/issues/22)、[#24](https://github.com/TekesApps/TekesAgentKit/issues/24)、#27；在现有 [PR #10](https://github.com/TekesApps/TekesAgentKit/pull/10) 继续实施。桌面、物理 IME、OS AX 仍不运行；只改 AgentKit Rust/验证工具，不改 ClientKit、Tekes、Kernel 或 TekesUI。

## 已知缺口

现有真实 `--sidechat-proof` 绑定首端 `host.runtime` 和 `host.side_chats`，是单端证据。多端 Host 的生产分发已有 endpoint-qualified `side_chat_sources`，相关多端 fixture 通过；它们不能证明 Pi 作为第二端时的真实发送、取消、回传、重连与跨端迟到完成。

## 实施边界

沿用固定 Native0.9.3/Pi1.3.0/DSH0.2.1-rc.2、本地合成 provider、隔离 home/data，以及当前六个已装配普通父 Run 的同一个离屏 demo。顺序明确为 Native→Pi→DSH，不能把 Pi 临时改成首端来通过验收。验证入口为 opt-in `--multi-sidechat-delivery-receipt PATH`，仅在显式 endpoint-config 和 offscreen 下使用。

新驱动必须经原 Sidebar/SidePanel/Composer 控件、原 `DemoCommand`、Host dispatcher 和原 `HostResult` completion handler。不能人工调用 owner.create/open/close 替代分发，不能制造 SideChat 或 RunBinding，也不复制状态机。可截留原异步完成用于受控迟到，但必须将同一结果交回原 handler。

## 验收要求

1. Native/Pi 的原 Create 生成独立 scratch scope，绑定实际对应 endpoint runtime；原 SidePanel/RunPort、Composer、Transcript 挂载。父 Run 及其他 endpoint 草稿保留。
2. 回到缓存 tab 后通过原编辑器公开 Focus 事件模拟用户聚焦输入框，不声称 tab 自动回焦；原编辑器合成 Preedit 不发送；Commit 与 Send 只投原 scratch，一条唯一输入及真实最终回复；明确它不是系统中文 IME 验收。
3. 再发送受控持有的本地 provider 请求，在原运行态经原 Cancel 取消，记录实际取消状态。取消未派发 ticket 不替代运行取消。
4. Summary 返回、原 tab 重开保留原 Composer IDs/草稿/controller；该端公开 reconnect 后原输入不重复、草稿保留、epoch 增加；原 Close 确认 discard 后退休权限和 pane。
5. Pi 的原 Create 完成受控迟到到另一端选中期间，仅合法缓存，不改变当前主/侧栏选择或焦点；真实 Send 完成时切到另一端，也不串历史、草稿或抢选择。
6. DSH 原 Create 保持 disabled；Press 不产生 fork/指针。unsupported 单独记录，不伪造 DSH scratch。
7. 原始收据包含 endpoint 顺序、父/子 scope、原输入计数、old/new epoch、实际取消 status 与各断言。Python 拒绝缩减端点域、改为首端 Pi、错误 scope、缺失取消/迟到证明、重复输入、epoch 未增加和误称桌面通过。

先做 CPU/组件/独立 consumer 与工具拒绝回归，再用当前普通程序跑真实三端离屏。复用 SSD shared-target，不新建 target；保存源码/命令/返回、版本、程序 SHA、成功及失败证据。#24/#22 的完整窗口、附件、视觉/AX、容器等其余要求保持原范围，单一 profile 通过不自动关闭整体 Issue；#53 严格 CI 不豁免。
