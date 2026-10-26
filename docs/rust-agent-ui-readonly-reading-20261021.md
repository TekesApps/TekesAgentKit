# 有界只读子历史的阅读事务

`ReadOnlyTranscript` 现在与主 Run 一样，通过当前精确公开 TekesUI `1294987dacabb33c0e411c13ee5ea544a1123f8e` 的原阅读事务发布内容。原生 demo 已接入，不再对已挂载的只读 pane 直接 `update`。

参考当前 ClientKit `ClientEmbeddedRunView` 的 per-scope pane 生命周期及 Tekes `ClientAgentHostAdapters` 的宿主边界；只读能力仍由 AgentKit 已发布的 `ChildPaneBinding`/bounded history 契约决定。这不是把不可写 Native child 伪装成独立可写 Run，也不代表所有 ClientKit pane 的视觉排版已验收。

## 接入

- 创建时先用 `ReadOnlyTranscript::update(input)` 配置初始内容，再挂载原组件树；用原 SDK `set_scroll_reading_document` 声明完整稳定 row IDs。
- 已挂载更新调用 `prepare_update(input)`，得到借用的 `ReadOnlyTranscriptUpdate`。它检查 scope、binding publication、epoch、完整 Transcript 和行身份；状态格式化在此完成，不在 SDK callback 中调用宿主 formatter。
- 保留原 `ReadingDocumentVersion`，同一 UI turn 立即调用 `publish_readonly_reading_update(host, root, &version, display_revision, &plan)`。
- `display_revision` 严格递增，独立于 provider/binding publication；loading、error、ready 可以改变控制区高度而不产生新的历史 read clock。

共享 adapter 在原 SDK 修改文档前完成整个 pane 的预检。root、原 owner、generation 和 Transcript 计划均须匹配；clone/foreign pane、availability ABA 或生命周期变化后的旧计划不能写入。SDK 仍验证原 attachment token。计划提前持有状态字符串；成功 callback 同时应用正文、状态、刷新和分页控件。没有自行计算滚动补偿、存像素恢复阅读、复制 selection/reading engine 或网络自动重试。

公开 `TranscriptContent::validate_prepared_update` 是此前 apply 预检的无修改拆分；原 apply 行为保持，供完整 pane 在 SDK transaction 前检查稳定 IDs。`ReadOnlyTranscript::update` 保留用于初始或无 reading attachment 的配置；挂载阅读文档后，宿主应走共享 adapter。

## 验证

- CoreText/公开 SDK：同 publication 的 loading 控件高度变化及历史前插保持原 item/局部阅读位置；旧 attachment 在正文/状态修改前拒绝；同值 ABA、相同 IDs 的不同 owner 拒绝；宿主状态 formatter 不在 SDK callback 重入执行。第二公开 consumer 调用同一 adapter。
- 原有11项只读子历史与主 Run 回归保留；完整 UI workspace、strict all-target Clippy、fmt与边界检查通过。阅读专项28项包含以上新测试。
- 新原生/GPU `--components --transcript-content --readonly-reading-native-selftest`：公开 owned SEP 的 bounded child，经原分页与刷新控件读取；渲染 rows 38→75，前插和刷新后的原 SDK item/局部锚点保持。主会话中文草稿不变、没有提交，只读 pane 无 Composer、评价或写操作。
- 同一 frozen native 程序重跑主 Run 分页/保留阅读/重连/rail/Jump 组字闭环及系统简体拼音软件探针；结果分开记录。文本服务/滚轮为工具合成，系统拼音使用进程定向按键；不宣称实体键盘、候选窗、VoiceOver、三真实harness的新只读pane锚点或配对ClientKit视觉验收。

证据位于 `.runtime-fixtures/agentkit-readonly-reading-evidence/`，程序为 `.runtime-fixtures/AgentKitReadOnlyReadingCheck.app`，包含原生及系统拼音日志/收据、源及程序哈希、组件测试与Clippy日志。所有构建复用 `/tmp/agentkit-layout-text-292-target`，debug0、incremental关闭；未新建target，删除缓存0字节，保留固定源、程序和证据。

新树持久 checkpoint 仍跟踪 TekesUI #42。完整 SidePanel/rail hover、富内容/selection、真实 Native bounded pane 的新原生阅读验收和最终跨Views对照继续开放，PR保持实验状态，不据此宣布完整UI完成。
