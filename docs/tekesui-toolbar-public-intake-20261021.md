# TekesUI 公开精确版本接收

默认实验依赖升级为 `09b4bab44b6dfa65487250ea816b610b9df59095`，来自已合并的 [TekesUI #43](https://github.com/TekesApps/TekesUI/pull/43) 与 annotated tag `remediation-toolbar-checkpoint-20261021`。核对时公开 main 也是此 SHA。tag object 为 `bfd94fe7585d286e3069dea530c31c5764de02f4`，commit tree 为 `a0f7c26878715d7ba6829c1222daee3b46f87ffe`。

匿名下载的完整公开归档逐项验证了全部 2760 个 Git blob。Cargo 实际消费八包、241 个输入，清单 SHA256 为 `44b3174022584d3c099a45b86892d17dbf948905476c0f038be104df12aefe46`。manifest、六个直接依赖及八个 lock source 同时更新；非 SDK lock 包未变。旧 1294987 清单保留在 `ui/baselines/`，未改写旧版本的验收证据。

## 消费边界

对照旧 pin，消费闭包只有 12 项输入变化；包括 ToolbarLabel、window toolbar 的字体角色、可选按钮 chrome 高度、原生标题栏测量和设计输入。默认普通按钮行为保持，AgentKit 不匹配穷举 TextRole，公开组件及第二 consumer 可直接编译。

[#22](https://github.com/TekesApps/TekesUI/issues/22) / [#28](https://github.com/TekesApps/TekesUI/issues/28) 仍为 OPEN，但需要的公开 reading/reveal 入口已在本版本中，不能再以 issue 状态解释为无可消费 API。原生 demo 继续使用原 SDK 的 attachment/update、稳定 item reveal、普通回合纵轴 Start 和最后回合 follow_latest；Sidebar 选择后自动 reveal 和独立 rail 复用这些入口。没有复制滚动引擎、反算 offset、以 focus 代替 reveal 或更换 Composer。

本次八包闭包不包含新的 ProcessContainer / app-host 集成，不把远程 View 的键盘、IME、AX 当成已有能力。AgentKit 的业务控件及 presentation 仍属于 AgentKit；窗口与产品布局属于宿主。没有修改 TekesUI、Kernel、ClientKit 或 Tekes 源码。

## 分层证据

隔离公开 Git consumer 的 432 项测试、strict all-target Clippy、原生构建通过；本 checkout 的最终 432 项测试、strict Clippy 和实际 Cargo 输入边界检查也通过。第二 consumer 在 workspace 验证内，不使用本地 SDK 路径覆盖。

隔离 consumer 的原生混合 Transcript 验证实际分页按钮 20→44 项、前插局部锚点、A/B 阅读及草稿恢复、重连、Start/Latest/Jump，导航期间原编辑器、焦点及中文 marked baseline 保持。系统简体拼音软件探针也通过：向独立 PID 发键，`ni hao` 组字，发送/导航屏障，空格提交“你好”，再显式发送；不更换用户输入法。工具合成输入和系统输入法集成分别记账，不提升为实体键盘、候选窗或 VoiceOver 验收。

完整证据在 `.runtime-fixtures/tekesui-toolbar-09b4bab4/`。`consumer-*` / `native-reading.log` / `system-ime.*` 属于隔离 consumer；`live-*`、`diagnostic-*`、`final-*` 与 `repeat-*` 保留本 checkout 的实际执行记录。原先误用系统 Python 后重复验证的旧 129 pin 结果单列 `old-baseline-extra-run/`，不计新 pin 验收。

最初 Pi 原生流式探针停在发送等待阶段，尚无充分信息确定原因；加入只读诊断后复测通过，日志明确一次提交、active preview、三次流内追加及结算/阅读/重连/草稿断言。另一次 DSH 流式探针和本 checkout 的首个系统拼音探针记录到窗口失去激活后失败；同期观察到其他 SDK 原生测试进程，但未证明因果，也未停止其他任务。失败程序和日志保留，不改断言或强抢焦点，独立窗口系统拼音复测通过。不能用成功复测抹掉这些失败记录。

DSH 随后复测通过真实原生分页 50→59 项、A/B 保留阅读及重连，并通过三次流内追加的阅读/中文 preedit/草稿断言。Native 新 pin 的阅读探针在会话恢复阶段失去激活，未形成完整收据，后续流式模式未执行，不能宣称本轮三端全部通过。最终程序的 local 阅读探针也分别在 phase 8/12 失去激活而失败；只读前台记录显示运行中前台从 demo 切回 ChatGPT，不以此认定 SDK 组件缺陷，也不强制抢回焦点。完整最终程序原生分页→导航→组字验收仍待安静窗口补测。

本 checkout 成功的 Pi/DSH/系统拼音诊断程序 SHA256 为 `8589c83445d1b31843bcde615a72b622b79ad4cdd602ab1de750a2ba43f2a062`，保留于证据目录的 `diagnostic-program/`。随后只把发送诊断的复杂 tuple 改成具名结构以满足 Clippy，最终程序 SHA256 为 `2248ec63615c6dea5586be5c43abe5bfee5bf99bbc10e68149b72d0010c43493`，见 `final-program.json`；432项测试和 strict Clippy 对应最终源码，不能把不同程序的原生收据混作同一个二进制验收。

本次继续复用现有共享 target，debug0、incremental 关闭，无新 target。确认无编译进程或文件占用后，用单一 Cargo 兼容 `flock` 清理隔离 AgentKit 快照的 21 个 rlib/rmeta，实际共享 target 占用减少 **56,860,672 字节（54.23 MiB）**。源码、固定归档、日志、截图、收据、原生程序及运行依赖保留，详见 `cleanup.json`。

UI 保持实验状态。销毁重建后的持久 reading checkpoint 继续跟踪 [#42](https://github.com/TekesApps/TekesUI/issues/42)；完整富内容、rail hover、配对视觉及物理输入法验收仍开放。本次没有发现足以独立复现并归因 SDK 的新基础组件缺口，不重复建立 issue，也不据此关闭 #22/#28。
