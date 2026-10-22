# 当前程序：原生与三端组合续验（2026-10-23）

本轮补齐共享组件基线 `f44f4ff` 的本地原生专项，并修复验收驱动后完成真实 Native/Pi/DSH 的协议到离屏 GPU 组合验证。完整 UI 交付仍未完成；PR #10 保持 draft，不合并、不打完成 tag。

## 程序与依赖

- TekesUI 仍精确锁定公开 `50ec49f3446924fdbc66b3cff22cd8cfe150fe7c`；公开 main 本轮复核未变化，不消费兄弟仓库 WIP。
- 原生程序 SHA256：`708f1d932de8b067d78237dbf59ac42c60fd8d2de53d0591e2246fe0d5c8a063`，原始输入见 `.runtime-fixtures/three-sep-current-20261023/programs.json`。
- 离屏程序 SHA256：`2f0dde99f86841828a2842a39e8c90bcb651cd9a5d325339a563249f7aeea7d3`；相较原生程序只修改 `history_proof.rs` 的后台命令处理及 offscreen 证明顺序，输入见 `proof-program.json`。不把前一程序的原生收据转移到后一程序。
- Headless 程序 SHA256：`93b956591657dd3361dc212689bd099f4fe6f9a702c9a4055cdae6e128835f34`。
- Pi 官方 1.3.0、DSH 官方 0.2.1-rc.2，精确 npm 锁与安装完整性记录保留在专用 runtime 根；复用 Node 24.20.0。Native 使用显式指定的已安装 Kernel 0.9.3 supervisor/worker，仅验证 UI-only 模式。
- 全部调用使用本地合成 provider、独立临时会话及数据根；未改个人会话、账户或其他仓库源码。

## 原生专项

通过正常 APPL / LaunchServices 启动，记录原始应用输出和前台 app 身份；不以 launcher 返回 0 代替应用内断言。

| 专项 | 结果 | 限制 |
| --- | --- | --- |
| 分页、阅读、焦点 | 原公开分页控件从 20 条增至 44 条；前插锚点、向上阅读、A/B 阅读恢复、草稿隔离、重连恢复、原编辑器与 marked text 保留均通过。 | 本地 fixture；滚轮与该专项的 preedit 来自 SDK selftest。 |
| 系统中文组字 | 当前系统简体拼音，向已核验 PID 投递键事件，观察系统产生 `ni hao` marked text；组字期间发送/导航屏障通过，空格提交 `你好`，最终单次发送通过。输入源未切换。 | 软件键事件；不代表物理键盘、候选窗、VoiceOver 验收。未使用粘贴或输入驱动直接注入 Preedit。 |
| SideChat 菜单 | 两个真实 scratch pointer、重复可见标签、当前项勾选、原编辑器保留、关闭移除及主草稿保留通过。 | 本地协议 fixture；普通宿主导航控件，不代表原生窗口 toolbar 或 ProcessContainer。 |

原始收据为 `current-reading.json`、`current-ime.json`、`current-sidechat.json`，对应日志及前台轨迹均保留。

## 真实协议到离屏 GPU

三端组合修正后连续完成两次，最后一次为每端保留原始 stdout/stderr、JSON 与明暗反馈截图。

| SEP | 历史来源 | 共同通过范围 |
| --- | --- | --- |
| Pi 1.3.0 | 原 nativeConversation | 28 回合、分页后 56 条消息；原 scope、身份去重、分页前沿、重连历史、B 草稿保留；反馈初读、嵌入 Transcript 的正向评价/中文备注/重开/删除、独立控件与明暗渲染。 |
| DSH 0.2.1-rc.2 | journal | 同上。 |
| Native Kernel 0.9.3 | journal，UI-only | 同上；不等于 0.9.3 完整生命周期、真实多消息有界子历史通过。 |

Pi/DSH 原脚本的会话恢复、生命周期及子代理控制断言也完成；具体 marker 保留在 `three-sep-retained.log`。Native 明确跳过完整生命周期。所有 UI 收据仍保留 `completeRunPort=false`、`completeTranscript=false`、`richContent=false`、`keyedReadingAcceptance=false`、`nativeInteraction=false`，不从离屏结果推导原生阅读或完整 Views 对齐。

最后一次原始 UI 收据分别为 `retained-pi.consumer.json`、`retained-dsh.consumer.json`、`retained.consumer.json`。程序、关键事实和证据哈希见[机器记录](rust-agent-ui-current-validation-20261023.json)。

## 修复与检查

第一次组合运行在 Pi UI 的“automatic initial feedback read missing”处失败，日志完整保留。定位出两处验收组合问题：历史探针筛选分页请求时丢掉事件附带的初读命令，导航返回的命令也未执行；独立反馈协议探针随后使 mounted frame 失效，却再断言 cold read。修正为按捕获来源执行附带初读、拒绝意外命令，并先证明 mounted 初读，再执行会有意推进反馈 publication 的独立协议探针。未修改共享组件状态机，也未放宽断言。

两个协议驱动不再猜测仓库 `target/debug`，而是读取 Cargo 的 `compiler-artifact.executable`；支持显式冻结 headless 程序并记录哈希。新增四项检查覆盖共享 target、测试产物排除、显式程序、歧义/缺失/构建失败拒绝。所有 harness 的原始 UI 输出均保留，包含失败输出。

本轮 Python 61 项、demo 反馈 14 项、demo all-target strict Clippy、格式与 Rust 依赖边界检查通过。f44 的完整测试计数仍单独属于此前记录，不将本轮专项称为全量重跑。

## 仍开放的门禁与资源

f44 两项 Linux CI 成功、两项 macOS CI 均在严格 `bounded_detail_reading` 退出 1；#53 仍开放。本轮未更改 SDK、严格检查或默认文档流策略。

ProcessContainer 的公开 50ec49f 源码再次核验：`container.rs::new_view` 不支持远端 keyboard/IME/AX，`ViewCapabilities::V1` 的 keyboard/scroll/text_ime/accessibility 均为 false。既有 #48 仍适用，不复制 IPC 或输入引擎绕过。富内容、glyph/紧凑动作、持久阅读 checkpoint、完整三端原生/布局与跨进程等仍按[交付清单](rust-agent-ui-delivery-status.md)推进。

复用 `/tmp/agentkit-layout-text-292-target`，关闭 incremental，未新建 target。本轮未删除缓存，实际释放 0 字节。保留约 249 MiB 的验收程序/日志/截图，以及约 993 MiB 的精确 Pi/DSH 运行依赖；后者保留锁文件和完整性记录，用于后续复验，不作为临时编译缓存删除。
