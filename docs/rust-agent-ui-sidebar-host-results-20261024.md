# Sidebar 宿主与并行 Issue 续批收据

源码提交 `4967d15`，PR #10。子会话按互斥文件推进 #22/#23/#25；主会话集中审查、修正、构建、提交与回填。只消费精确 TekesUI `50ec49f3446924fdbc66b3cff22cd8cfe150fe7c`，未改 SDK、Swift 或 Kernel。

| Issue | 实现提交 | 本批结果 |
| --- | --- | --- |
| #22 | `4967d15` | Single/Multi adapter 接入 ComponentDemo/native dispatch；原 endpoint runtime 路由、不可变 owner publication、旧导航/action/pending退休，非匹配 SideChat 明确禁用。 |
| #23 | `9d7c00d` | 新增 ready/loading/error/missing 内容 token 退休，以及替换 child source 后旧读取错误拒绝回归。未发现需改写状态机的确定缺陷。 |
| #25 | `7bfdd61` | 公开 Summary facts 校验/逐行过滤、无效 scope/空身份拒绝、host-projected namespaced parent alias 被动化，保留合法显示 metadata 与 sibling。 |

## 验证层级

- presentation 全套 226/0（alias 补丁前）；最终 panels 16/0，严格 all-target Clippy 通过。没有声称最终源码再次全套执行。
- UI workspace 637 passed / 0 failed / 1 项既有忽略（#49）；随后只收紧 publication 可变性，最终宿主定向回归 1/0，并通过最终 workspace all-target 严格 Clippy。最后只按 Clippy 建议折叠等价条件。没有声称可变性修改后重复完整 suite。
- 独立外部 public consumer 实际编译运行，验证构造/ContentHost 和新公开 pending 退休入口；固定 SDK 243 份源码逐个 hash 一致。它不证明业务交互、真实窗口或 IME。
- generator Python 3 项、格式与 Rust UI 静态边界检查通过。构建复用原共享 target；没有创建 target、没有删除缓存，实际释放 0 bytes。

失败尝试日志仍保留：旧 native query API、测试导入/移动值、fixture endpoint ID 和把菜单模式当按钮模式的问题由主会话修正。最终宿主测试直接捕获真实共享定义、检查原 fixture IO 和生命周期门禁；没有把合成菜单点击或近似文本渲染当作 UI 验收。

两套既有 ddc49c7 CI 的独立 consumer/headless/UI tests 均成功，仍只在 TekesUI #53 严格 nested reading 失败；见[CI 收据](rust-agent-ui-sidebar-host-ci-20261024.json)。新推送的 CI 需单独看新 head，不转移旧步骤结果。

## 尚未验收

跨端全局搜索、真实多 harness 同窗配置、过滤分页/reveal 整体组合、observer 同实例恢复与非首端 SideChat 完整能力仍开放。测试中的 reveal 是目标身份和恢复投影，不是桌面像素可见性。#23 native ChildOpened(Err) notice dispatch、完整窗口/chrome、系统中文 IME、AX、ProcessContainer、性能与完整 ClientKit parity 均未据本批通过。未启动任何桌面验收、截图、TCC 或新应用身份；宽范围 Issue 保持开放，不合并、不打 tag。

精确日志路径、hash、分层范围见[机器收据](rust-agent-ui-sidebar-host-results-20261024.json)。此前真实 Pi/DSH 程序和原始证据仍保留独立身份，没有把历史收据转移给本次源码。
