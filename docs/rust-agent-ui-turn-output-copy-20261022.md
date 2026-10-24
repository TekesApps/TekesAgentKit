# 回合输出复制与运行显示边界

本次对齐 ClientKit `AgentKitTranscriptView.Transcript.changes` 的回合 output 投影、`TranscriptMetalAppBridges` 的显示门槛，以及 `TranscriptTurnFootnote.rawOutputNodes` 的复制入口。Tekes host 继续复用 ClientKit 的 Run 内容，窗口/导航/系统适配归宿主。Rust 侧没有修改这些参考仓库或 TekesUI。

## 输出文本与复制来源

`TranscriptTurnCopy::read(snapshot, turn_id)` 从原 scoped snapshot 读取已知回合；无效文档或未知回合拒绝。有 assistant `is_final` 事实时，将该回合所有 assistant `text` 按 snapshot 顺序以换行连接。空段保留分隔符，不把 reasoning、tools 或其他角色拼入输出。聚合后为空则没有文本复制入口。

这与 Source turn output 的投影一致，包含源 assistant 的 hidden/provisional 文本，而不是擅自更改其顺序或筛选规则。它不为隐藏项新增独立可见行，不把临时 timeline ID 当成 Native durable entry，不构成最终响应或执行授权。没有正式 answer 事实、孤立项或其他 turn 的文本不能生成此回合复制入口。分组在一次 publication 的计划中完成，避免逐回合全量扫描。

`TranscriptCopyIntent::source()` 区分：

- `TranscriptCopySource::Message(identity)`：原单条消息复制，`message()` 返回 Some。
- `TranscriptCopySource::TurnOutput { turn_id, item_ids }`：完整源回合聚合；item_ids 是原 snapshot 中的 item ID，按相同顺序捕获。`message()` 返回 None，不伪造一个“代表整个回合”的消息 ID。

UI 仍为未发布实验包；`message()` 从引用改为 Option，宿主需迁移该 introspection。系统剪贴板路径继续只消费捕获的 `text()`，一次 `claim()` 后写入；不能查询新选中的会话。clone 共享一次写入认领。来源集合、文字或 epoch 改变时退役旧复制按钮；同一 publication 下的冲突在改控件前拒绝，已捕获 intent 保留原来源与文字。

内部同时捕获 source position、record 和 stream 来源来隔离旧事件，不能因临时 item ID/文字相同而把另一 Native entry 当成原来源。该校验不把 source ordinal 变成 durable identity，也不向 clipboard intent 授予 reading 或 endpoint 权限。

标准 Transcript / RunPort 和只读内容自动走同一聚合路径；公开 `TurnFootnote::update_with_turn_copy` 支持独立挂载。旧 `update_with_output` 仍是显式单 final item 的低层兼容入口，保持原校验，不作为标准回合复制语义。

`TranscriptContent::copy_action_ids` / `RunPort::copy_action_ids` 提供本帧原复制动作 ID，隐藏 Run pane 不返回目标。demo 只在当前组字 Composer 对应 RunPort 中，按原 SDK 可见 frame 放行这些保焦点复制按钮；AX 只放行 Press，不授予 Focus。实际 hit、capture、动作与焦点仍由 SDK 处理。RunPort 将本地 clipboard 意图置于 Composer preedit 业务屏障之前，send、feedback、turn/page 等权限继续独立判断；不将白名单扩成通用点击放行。

## 与反馈、执行和 live 显示分离

`TranscriptSection::final_response` 和 feedback target 策略保持不变：completed、唯一 formal candidate、可见且非 provisional 等条件仍独立校验。因此 failed/aborted 的正式 output 或多 final 聚合可以本地复制，却不会因此获得 feedback、fork、execute 权限。available/offline 不禁用已有本地复制。

ClientKit 明确指出 answer availability 不等于 turn settlement。只有最新实际回合且原 Run/连接/child 可继续的条件满足时才取得 live owner；正式 final item 先到不会结束运行脚注，旧的未结束回合也不能越过较新已结束回合取得 live owner。live owner 期间继续只显示运行信息，复制及其他 settled 控件隐藏，直到原结束事实/显示状态变化。原时钟、reading transaction、编辑器、焦点、组字与 wake 所有权保持。

## 验证范围

组件与第二公开 consumer 检查原顺序、空段、隐藏文本、角色排除、无 final、未知/重复身份、failed/aborted/multiple-final、原 scope 捕获、publication 拒绝、来源更换后的旧事件拒绝、原 footer 保留及一次认领。Run 阅读测试检查 final-before-end 的 live owner 和较新回合对旧 live owner 的隔离。

原生计时模式继续通过 owned SEP facts → presentation → 原 RunPort 更新，不另写状态机。模式扩展两段正式 journal message、结束前 live、结束后原复制按钮和 SDK 私有测试剪贴板读回；保持原 Composer owner/focus/selection/marked baseline。Journal 在 turn/end 才标记 final，因而 native journal 模式不冒充“is_final 已先到”的协议证据；该情况由独立组件测试覆盖。真实 harness、软件输入法、实体键盘、跨进程 UI 等结果仍需分别记录。

首次原生程序停在复制阶段，未计为通过：它暴露 demo 的 preedit 指针白名单及 RunPort 的早返回均漏了原本地复制。新宿主测试通过原 pointer/AX Press 复现并覆盖修复，AX Focus 仍拒绝；初始程序、失败日志和测试开发中的编译/断言失败保留，最终程序的范围另记。

既有 RunPort 测试曾规定组字时连本地复制也一律阻止，完整回归因这一旧预期失败。现改为验证原复制可用、焦点/marked baseline 不变且发送仍拒绝；没有删除测试或放开业务提交。

本阶段共有493项组件/第二 consumer 测试通过；随后只修正 native 夹具/probe 的时间和断言，final all-target Clippy/build 通过。31项工具测试、fmt、静态边界及实际 Cargo 八包241输入校验通过。最终提交的完整 CI 单独核对。

最终程序 `.runtime-fixtures/AgentKitTurnOutputCopyFinal.app`，SHA256 `842bb1eb253388113ccca6050c004ffcd40d8693711572aca8a68656636350ec`：

- 原生计时/整回合复制：owned SEP 两段正式消息、结束前 live、结束后时间与 wake、原指针复制和私有剪贴板读回通过；原 Composer owner、焦点、选区/marked baseline 保持，无提交。截图已检查；不是配对 Source 视觉验收。
- 原生阅读 fixture：分页20→44、前插锚点、向上阅读、Start/Latest、A/B独立草稿与阅读、重连及原焦点/合成中文预编辑保持通过。
- 原生只读历史 fixture：38→75项，前插/刷新锚点和主草稿保持通过。
- 系统简体拼音：独立 PID 发键、组字/确认/发送屏障通过，前后输入源均为 `com.apple.inputmethod.SCIM.ITABC`。这是系统软件输入法证据，不代表实体键盘或候选窗视觉验收。

中间程序 `4d1ea0c3…` 的实际 Pi1.3.0 原生分页50→56、阅读恢复、A/B草稿及重连通过；该 real-history 模式不执行组字/原生分页按钮/新 rail 操作，其 false 字段保留。随后 real Pi stream 模式失去窗口激活并停在 phase2，未通过；DSH/Native 的 native stream/controls 模式未执行。最终程序的真实三端协议/offscreen 另记收据，不能用 protocol/offscreen 或此 Pi 阅读证明完整三端原生交互通过。

最终 `842bb1eb…` 程序已通过 Pi1.3.0、DSH0.2.1-rc.2、Native Kernel0.9.3 的真实协议、历史/分页回传和混合组件到 GPU 的 offscreen 验证；三端各有28回合/56项内容。provider为隔离本地受控服务，Native 显式 UI-only 不覆盖完整生命周期/archive。它们不是 UI 跨进程容器或真实三端原生组字/流式的新通过记录。

截图检查发现中间计时夹具漏设真实结束时间，最终程序恢复 Unix 毫秒时钟并新增 end≥start 断言。中间程序及其截图/日志保留，未改写为最终验收。跨进程 ProcessContainer、VoiceOver、完整布局及跨 pane 的全部辅助动作组合仍未验收，PR #10 保持草稿，未合并/打 tag。

最终程序、日志、源码和验收范围见[同名 JSON](rust-agent-ui-turn-output-copy-20261022.json)。完整单行 aside/图标、note popover、配对布局和 ProcessContainer 迁移仍未完成，分别保留既有上游问题和目标；不能以本次本地复制对齐代表整体 UI 完成。精确 TekesUI pin 保持 `09b4bab44b6dfa65487250ea816b610b9df59095`，复用既有 target，保留 WIP、快照、失败日志及验收程序。
