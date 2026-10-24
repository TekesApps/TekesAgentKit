# #25：真实 Native Task 来源与原 Summary 被动行

关联 [#25](https://github.com/TekesApps/TekesAgentKit/issues/25)、#23/#27、[PR #10](https://github.com/TekesApps/TekesAgentKit/pull/10)。[接入/来源契约](rust-agent-ui-native-summary-source-contract-20261024.md) · [机器收据](rust-agent-ui-native-summary-source-results-20261024.json)。只消费公开精确TekesUI，不改任何上游；UI仍试验，Issue不关闭，不合并/tag/发布。

## 本切片结果

- 复用858a63c的既有feature headless（SHA256 `2ebad9784377de4e55a890df6478db2ac22a8b16c06fca2a988c4ab37d7ac022`）、固定已安装Kernel0.9.3与隔离合成loopback provider。15条公开命令、3次provider请求。没有重新构建Kernel或调用fork；ownedKernel最后退出0，之后才写accepted运行回执。
- 真实Task seed、childCatalog、公开bounded page(maxMessages=50)、重连前后childHistory通过。它们维持各自父/child身份和read-only约束；不能把bounded子历史当作Summary Task独立pointer。
- 同次原publication的Summary Task名称、scope、status与原tool.arguments/phase一致；当前来源是Journal call seq4及result seq5。按原位置匹配原message/callId，不拆opaque工具ID。原status=done，原AgentTask策略显示completed。
- 显式 `--passive-tasks` consumer将该原facts送SummaryView→ContentHost，原read-only TextFieldContent标题挂载，Task仍在父scope、无独立Run与导航action；只记录原status策略，没有绘制glyph或像素。默认Pi模式仍要求actionable child，并已用前一份真实Pi见证回归通过。
- Python197/0（新增8项来源/拒绝测试）、consumer example严格Clippy和格式通过。未重跑未修改的747项UI全套；静态核验共享包/宿主闭包137文件SHA与858a63c独立consumer冻结收据完全一致。这不把旧CI或测试扩报为新切片的全量运行。

## 证据与限制

SSD `.runtime-fixtures/native-summary-source-20261024/` 保留commands、原stdout/stderr、版本/程序与providers快照hash、原witness、bounded来源、原组件收据、退出收据及失败源码。首次校验器只接受NativeConversation，实际Kernel返回Journal而失败；该失败保留，修正方言校验后最终完整重跑通过。最终consumer程序已归档，headless复用已归档同hash程序；所有历史程序仅归档。

Kernel诊断保留了未配置DeepSeek模型导致的自动标题细化失败；本profile不配置或验收自动标题，没有把它算成功，也未修改Kernel来掩盖诊断。Task调用/有界历史/最终同帧来源与宿主退出检查独立通过。全部使用合成provider，没有付费模型。

真实running/failure转换、同Task行动态pointer、此profile父草稿隔离、最终glyph/hover/selection/窄宽字号主题、桌面/系统IME/OS AX及ProcessContainer仍未验收。服务端多流事务一致和对完整伪造producer的抵抗力未证明。不能据此关闭#25/#23/#27或清除TekesUI #47/#53等门槛。

本切片没有增加target或重跑桌面，只复用SSD共享target；新运行目录是小型验收证据，保留。本切片新增删除0字节；前一来源切片清理的196.4MiB增量缓存不重复计入。上游和其他任务WIP、进程均未更改。

上一提交858a63c的PR40089263375/push40089261151原日志与artifact已核实：默认root341/0、opt-in249/0、UI747/0/1忽略与独立consumer执行成功；唯一失败为严格#53（accepted=false，父offset追加后634、detail不可见）。它验证858a63c，不能作为本Native切片新CI；本切片CI待推送后运行。
