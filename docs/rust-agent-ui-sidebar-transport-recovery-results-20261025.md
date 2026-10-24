# 三端 Sidebar 真实连接故障与恢复（2026-10-25）

归属 #22/#27，关联 #15/#28、draft PR #10。基线 `29ea2e1`。范围与原回调契约见[设计](rust-agent-ui-sidebar-transport-recovery-contract-20261025.md)，精确程序与原收据 SHA 见[结构化结果](rust-agent-ui-sidebar-transport-recovery-results-20261025.json)。这次只修改 AgentKit，未改 TekesUI、Kernel 或 ClientKit，未启动桌面。

真实三端场景暴露并修复了 Pi adapter 的关闭缺陷：原 `close()` 已结束 RPC，但 inventory/control/workspace/actionables 等流仍等待 Context notifications，Runtime 不会进入连接失败预算。现在所有 endpoint-owned FrameStream 监听既有 lifetime，关闭时返回一次 `Disconnected` 后结束；关闭后拒绝新订阅。没有新增恢复状态机、伪造 Failed 或重发业务命令。回归先确认五个 `next()` 已 pending；旧实现实际超时失败，窄修后同一回归通过。最初测试借用错误导致的编译失败另留 `pi-red.log`，只以 `pi-red-2.log` 的实际 pending 超时作为红证据。

最终 `final` 运行以真实 Native 0.9.3、Pi 1.3.0、DSH 0.2.1-rc.2，同时加载九个公开创建的会话和六个原 RunBinding。先经原 Show more 展开九行；逐端封闭 connector gate 并关闭 Runtime 实际持有的原 endpoint，四次真实拒连后进入 Failed；开放 gate 后通过原 ComponentDemo Reconnect 控件的合成 Content Press 获取命令，交回同一原 Runtime。它是无窗口验证宿主，不是私有 NativeHost 窗口循环。

三端独立核验均通过：

- 每端 connection epoch `2 → 11 → 13`，新 endpoint 对象与公开库存/原历史均恢复；只看到 Ready 不算通过。
- 故障端三行保留、activity.isStale=true，其余两端仍 fresh 且真实 inventory refresh revision 前进、epoch 不变。内存 observer 始终 observing/converged，generation=1，observer 自身 stale=false，不出现 Retry。
- 原 Sidebar/host/controller 保持；九行实际 ContentHost 挂载及地址对应 row ID 不变，选中 scope 和六份非空中文草稿保留。这些草稿通过 API 设置，不是物理 IME 验收。
- 故障端旧 selected-read 因来源 epoch 变化被拒绝；两个健康端旧回执仍可用。这里使用 probe 自有 read-scope generation=1，不代表验证了 ComponentDemo 私有导航代数，也不冒充网络迟到帧。
- 故障前捕获的原 Copy 菜单 definition 在 Failed 时拒绝，恢复后由原 host 依当前 publication 捕获票据、重验并首次返回原地址/原 canonical text；不声称跨重连保存并提交了同一份旧 action ticket，也不写 OS 剪贴板。
- provider 请求在 seeding 后与恢复后均为 9 次、路径完全相同。只证明本流程未新增模型请求；没有未知 mutation 结果重试、真实模型理解或付费模型结论。
- probe exit=0、其独立进程组已退出；owned Native/DSH exit=0。程序原路径、归档 SHA、公开运行时入口 SHA、阶段 stdout、最终 JSON 与失败记录都保留在 SSD。

最终 probe SHA256 `a4c255435ace8066ea0c76ef3dd2c138fa01a243deaa05054cbf4d0f7315d096`，headless SHA256 `7c10485cab7a4ed6fa4dd2865a9dcc83db8cf9a03d6742723d7f814d012b3247`。584 项源码/配置冻结并与待提交工作树核验；提交后另写 commit binding。早期失败与最终收据不互相覆盖。

验证：Root 402/0/0；UI 781/0/1（1 项既有忽略）；Python 307/0；Root/UI all-targets 严格 Clippy、格式、probe/headless 构建和边界检查通过。独立公开 consumer 的138项输入与此前实际通过的 `29ea2e1` 完全相同，本地没有重跑；本轮新 head CI 与其当前执行结果另行核对。TekesUI 精确 pin 保持 `50ec49f`，8包243文件闭包不变。

严格嵌套阅读 TekesUI #53、Native busy queue Kernel #86、上下文菜单/跨新树 checkpoint、完整 Views/ProcessContainer、桌面/系统 IME/OS AX/性能等原门槛保持开放；本轮不合并、不 tag、不关闭完整 Issue。没有新增 target；本轮缓存删除0字节，正在复用的共享 target 与原验收证据保留。SSD 迁移的历史收据另存 `migration-20261024`，不把历史空间净变化算成本轮释放量。
