# Activity 展示规则：共享判定与下一步容器边界

本阶段基于 `f61cfcb`，继续推进 ClientKit 对应 Views；完整交付仍未达到。精确 TekesUI pin 保持 `50ec49f3446924fdbc66b3cff22cd8cfe150fe7c`，未修改基础组件或依赖。

`TranscriptTurnPlanes.activity` 新增共享 `TranscriptActivityPresentation`，由既有 `TranscriptSection::planes`、`RunSnapshot::turn_planes` 和安装后的 `TranscriptContent::turn_planes` 返回。宿主和第二 consumer 转交事实，不另写展示状态机。它不是发送权限、读取凭据、展开状态存储或新 publication。

| 共享判定 | 来源与行为 |
| --- | --- |
| `Working` | 捕获的 live owner；候选 final 不提前终止。组件继续使用原 TekesUI 被动状态行。 |
| `Settled { default_expanded }` | 正常 completed 默认折叠；其他终止状态有 Activity 时默认展开；无 Activity 不产生空 disclosure。此字段是默认值，不覆盖用户展开选择。 |
| `BetweenInputs` | 已知展示顺序中，机器内容后出现用户输入；后续物理布局应保留输入之间的 Activity 时间线。 |
| `Direct` | 未结算且无 live owner、缺 turn、输入无已知位置、明确 `task-source:`，或开头 system 记录缺少 prompt-role 证据。保留原内容，不伪造历史结算或隐藏来源。 |

规则对照 ClientKit 的 `TranscriptMetalActivity.remainingNodes`、`defaultExpandedDisclosureTargets`、`TranscriptTurnPresentationPolicy.defaultsActivityExpandedAfterTermination` 和 Tekes 现有共享 Kit 宿主接法。正常/异常默认展开直接采用 `status != completed` 的源规则，不自行枚举一份“失败状态”。输入位置使用原 `display_order`，不改协议序号、身份或阅读锚点。

该阶段 Rust DTO 尚无 ClientKit 的 `systemPromptBeforeInput` 角色，因此保留 Direct 路径而不猜正文。当前 journal prompt 和开场输入的显式来源接线见[来源与显示](rust-agent-ui-system-prompts-20261023.md)；旧 DTO 无事实时仍保留原路径。这属于 AgentKit 业务契约，不是 TekesUI 控件缺口。

本次真正改变的控件行为只有 direct task-source 不再额外插入 Working；原消息、工具和编辑器仍在原树中。**尚未**物理拆分 OutputMessage 的 reasoning/activity 与 final body，也没有接入 settled disclosure 按钮。要完成布局，必须让原 Content 仅有一个 owner，在结算/折叠/展开时保留原 editor、选择区、焦点、局部阅读位置和 prepared-plan 门禁，不能复制 widget、增加影子状态机或用坐标重叠代替分区。

根 workspace 289 项、UI workspace 591 项通过，UI 保留一项既有 #49 忽略；两边全 targets 严格 Clippy、格式和 Rust 依赖边界检查通过。Section 引用其他 turn 的事实会拒绝。测试和精确输入摘要见 [机器记录](rust-agent-ui-activity-policy-20261023.json)。根 presentation 覆盖正常/异常/空 Activity、live candidate、retained running、缺 turn、task-source、追加输入、未知位置和 initial system；demo 与第二 consumer 共用同一个已安装 getter，并验证原内容保留。组件/宿主测试不代表原生、系统拼音或跨进程验收；上一阶段 3e0eeb6a 程序的原生收据保留原身份，不转移到本阶段源码。

上游公开 main 本轮复查仍为固定 50ec49f；#48、#53 仍开放。严格嵌套阅读 CI 门槛保留，PR #10 继续 draft。复用既有 target，未新建 target、未删除共享缓存；源文件、失败日志和验收程序均保留。
