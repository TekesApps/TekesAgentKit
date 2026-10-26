# 当前固定 UI 的真实三端原生验收与运行版本证据

UI 源码 `2137e514f3475d8d3317ae057797f65527d2b3cd`，同一冻结程序 SHA256 `bd1da513183def671b33ee0b9362a4281ab5b05787ef1ffb8a0412e601dc20be`。TekesUI 仍精确固定公开 `09b4bab44b6dfa65487250ea816b610b9df59095`，未修改任何 SDK、Kernel 或客户端参考源码。

## 实际结果

| Harness 的实际版本 | 原生公开分页 | 流式阅读 | 输入与执行 |
| --- | --- | --- | --- |
| Kernel 0.9.3 / v4 | 50→56 项，前插锚点、A/B 保留阅读和重连通过 | 3次 provisional 追加，旧 item/局部锚点与原编辑器保持，最终文本精确 | 中文 marked/focus 保持，单次提交；取消、失败显示、显式恢复与重连不重放通过 |
| Pi 1.3.0 | 50→56 项，同上 | 3次追加，同上 | 同上；完整协议、模型、子代理、queue、文件/附件等原脚本断言通过 |
| DSH 0.2.1-rc.2 | 50→59 项，同上 | 3次追加，同上 | 同上；完整协议、模型、独立子源、文件/附件及生命周期等原脚本断言通过 |

三端均使用真实 runtime/协议及独立临时 HOME/会话，provider 仅为本机受控服务，没有付费模型调用或用户配置修改。没有解析用户私有历史数据库。所有窗口按顺序运行，无并发抢焦点，也没有强制激活或降低断言。三端流式阅读截图均已检查：旧历史仍在视口内、用户内容/footnote居右、机器内容居左，Composer保留中文marked文本；没有ClientKit同内容配对视觉证据。

实际服务端分页检查在原 binding 上显式派发原请求，验证已有 editor focus/文本/选区在数据发布时保持；此模式的 `nativePagingControl`、`syntheticPreedit`、`opaqueBaselinePreserved` 明确为 false，不能据此声称分页按钮激活时保留不透明编辑 authority。原按钮与 Start/Latest/Jump+中文 marked 专项属于同一冻结程序的上一阶段 local fixture 收据，见 [SidePanel 布局记录](rust-agent-ui-sidepanel-layout-20261022.md)。本次真实流式专项另以 synthetic wheel/preedit 验证 marked baseline、锚点及焦点。

同一程序另通过系统简体拼音软件探针：PID定向键事件生成 `ni hao`，组字期间发送/导航屏障，空格提交“你好”后仅显式发送一次；输入源前后一致。这是系统输入法软件集成，不代表实体键盘、候选窗或 VoiceOver，也不声称三端各自使用实体中文输入验收。

## Native 版本纠正与未通过项

开始时复用 `/Users/frankqin/dev/TekesKernel/target/debug`，其实际 `--describe-build` 是 `0.9.0+web-auth.dev`，不是可按目录名推定的发布0.9.1。该次虽然三组原生专项通过，仍只保留为开发程序证据，不记发布版验收，也不把此前不同程序的历史收据改写为本次版本。

随后使用已安装发布版0.9.3的 supervisor/worker，**仅只读消费程序**；`--describe-build` 的 source_revision 为 `09508a9083c24c7accefc99516739f279b85913e`，与公开 annotated tag `v0.9.3` 的 peeled commit一致，握手也为0.9.3。程序SHA与原日志保留。

完整协议脚本在已完成一次回答、完成rename后调用 `session.archive`，返回 `session-running: Session has a running worker`。该次完整生命周期测试失败，未重试、未偷偷取消worker、未改Kernel或将失败判为通过。对照当前ClientKit `SessionEndpointRuntime.archiveSession` 和 `TekesClientRuntime.archive` 也为直接公开调用，没有可移植的“先停止worker再归档”规则；原因与发布版生命周期的完整验收仍开放。未据单次guard拒绝将其公开archive capability改成不支持，也未先行归因为Kernel缺陷。

为继续独立验证UI，新增显式 `--kernel-ui-only`：只创建隔离会话后进入原UI探针，打印 `NATIVE_LIFECYCLE_NOT_RUN`，不授予完整协议/生命周期/有界子历史通过。默认完整流程保持原断言与失败路径；不会在错误后自动切换此模式。上表Native三组UI通过来自这个显式模式，不能消除上述失败。

## 可重现输入与工具

`scripts/test-real-session-endpoints.py` 新增：

- `--kernel-supervisor PATH --kernel-worker PATH`：成对指定既有程序，与 `--kernel-target-dir` 互斥，不重编译/复制Kernel。
- `--kernel-expected-version 0.9.3`：启动前检查 `--describe-build`，所有headless握手再次检查精确版本。版本错误在启动 fixture 前拒绝。
- `NATIVE_RUNTIME_BUILD_RECEIPT` 保留程序hash/版本/source revision，各端 `*_RUNTIME_HANDSHAKE_RECEIPT` 记录实际公开版本。
- `--kernel-ui-only` 只供明确独立UI验收，必须配Kernel、demo和正式components；不接受有界子历史模式，不自动降级。

调用其它参数仍为原 `--ui-components --ui-transcript-content --ui-history-pages --ui-native-reading --ui-native-stream-reading --ui-native-run-controls`。Pi/DSH传已有固定runtime root；UI传冻结程序，不新建target。Kernel fixture provider配置来自只读v0.9.1源码，运行程序为明确指定的0.9.3，二者分开记录，不用源码HEAD推定二进制版本。

新 provenance helper 的5项测试覆盖精确版本、开发版拒绝、成对/互斥路径、陌生/非对象/缺版本描述及文件hash；全部脚本31项测试、Rust静态边界通过。本阶段无Rust UI代码变化，不重复编译UI；445项UI测试及strict Clippy归属2137e51的上一阶段。2137e51两组CI的四个job已全部SUCCESS。

证据位于 `.runtime-fixtures/three-real-2137e51-20261022/`：`receipts.json`、`programs-and-inputs.json`、`kernel093-ui.*`、`pi*`、`dsh*`、`system-ime.*`，并保留旧开发版、完整0.9.3失败、原脚本快照及公开tag/source证据。验收程序保留在 `.runtime-fixtures/AgentKitPanelLayoutCheck.app`。

继续复用原headless target与已有UI程序；无新target或SDK/Kernel构建，没有删除用途不明的共享缓存，实际删除字节0。源码/WIP、固定依赖、失败记录、收据、截图及程序均保留。PR #10仍为实验草稿；完整同名Views视觉、上游基础缺口、Native生命周期等未收口，未合并或打阶段tag。
