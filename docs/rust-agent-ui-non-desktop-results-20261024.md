# #15 / #19 / #27 非桌面验证补充（2026-10-24）

本轮不修改 Rust UI/SEP 实现或 TekesUI。源码基线 `8335231`；冻结程序、387 项 Rust/Cargo 输入、原始收据摘要和命令见[机器记录](rust-agent-ui-non-desktop-results-20261024.json)。记录不替代先前 `58521a1` 的矩阵与 CI 身份。

| 本轮范围 | 实际结果 | 不代表 |
| --- | --- | --- |
| #15 独立 package | 直接构造 RunPort/Sidebar/SidePanel 并转 ContentHost，`cargo run --locked --offline` 退出0；5个本地包、6个精确SDK包及243个审查输入核验。 | 原生窗口、ProcessContainer、完整业务交互。 |
| #19 来源 | 27个源码文件与原提交哈希一致；26项报告/计数/旁白策略和3项公开工具consumer回归通过。三端report/group fallback仍unknown；host diff是独立当前快照。 | 已找到真实报告或独立旁白producer。 |
| #27 三端协议/离屏 | 同一冻结UI程序：Native 0.9.3、Pi 1.4.0、DSH rc.2 的协议侧历史/locator/文件/发送/回传/重连及常规组件数据→离屏通过；Native完整协议与有界历史专项通过。 | 每种组件交互、完整Transcript/富内容、阅读锚点或ProcessContainer验收。 |
| #27 有界pane | 原source-call只读详情、只读控件、读取intent、分页可用性、重连、非空父草稿保留和拒绝发送通过。 | `readingAnchorAcceptance`仍false；没有运行native-readonly窗口探针。 |
| #27 SideChat | Native/Pi原SidePanel/RunPort/editor的创建、合成preedit不发送、提交、草稿切换/重开、重连无重复及确认丢弃通过。DSH仅不可用门禁/no fork通过。 | 系统输入法/物理键盘、原生阅读、跨进程；DSH完整SideChat。 |
| 工具与记录 | 92项Python测试、矩阵默认检查与本地程序/原始收据/Git源码哈希复核、UI/headless边界及diff检查通过。 | 整体CI通过或完整目标完成；#53严格失败不豁免。 |

UI程序 SHA-256：`2130113fb861b165a9a64bf1d5e0dff472c374058bce54557ddd4b1258d85c60`。
Headless SHA-256：`c41a0f74b775c200fa4543c08b669b61ccbbfd5a2d2ebdff7662230f11e08487`。
程序分别用APFS写时复制、独立inode归档；运行后复核全部来源输入和程序哈希未变。

普通+有界profile做了Native完整协议驱动；独立SideChat profile显式使用`--kernel-ui-only`，不能据后者补报Kernel协议通过。两次驱动都没有任何`--ui-native-*`参数；没有启动原生窗口、签名/安装宿主、系统截图/权限申请、系统IME、VoiceOver或ProcessContainer。PNG仅为程序自己的离屏渲染输出。

构建复用 `/tmp/agentkit-layout-text-292-target`；独立consumer是单独源码package，不是独立target。验收程序/锁文件/日志/JSON/PNG保留在 `.runtime-fixtures/issue-batch-20261024/` 和 `.runtime-fixtures/issue15-contract-20261024/`。没有删除缓存或历史证据，实际释放0字节；本轮没有创建新target。

#15仍待正式AgentKit UI不可变发行契约；#19仍待公开真实producer/fixture；#27仍待系统IME、窗口/布局、阅读锚点及同/跨进程完整验收。当前所有通过均限表列范围。
