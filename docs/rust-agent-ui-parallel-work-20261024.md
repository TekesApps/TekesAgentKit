# AgentKit 主会话与子会话并行安排

基线：`52288a0`，PR #10；遵循本仓库 AGENTS.md 的 Issue 驱动流程。本文是工作分配，不是验收完成声明。

| 工作 | 负责人 | 本轮可独立交付范围 | 文件写入范围 |
| --- | --- | --- | --- |
| #22 Sidebar | Sidebar 子会话（复用 issue15_contract） | 按既有设计增加固定多 endpoint 聚合、来源路由与非桌面回归 | 新 sidebar_multi binding/test，selected-read 复用，bindings module/export |
| #23 SidePanel | SidePanel 子会话（复用 issue27_matrix） | Task/child 状态、pointer 替换、父导航/迟到门禁核对；确定缺陷才修 | side_panel*、task/child binding、专属测试和审计 |
| #25 Summary | Summary 子会话（复用 issue19_sources） | sections/usage/metadata、来源动作和 slots/callbacks 核对；确定缺陷才修 | summary*、专属测试和审计 |
| #14/#15/#27/#28 | 主会话 | 接口决策、CI、范围协调、统一构建、代码审查、公开 consumer 与证据核验、Issue/PR 回填 | 共享索引、接入说明、CI/收据、集成改动 |

子会话先说明具体发现/API方案再修改；不跨写其他子项，不提交或推送。涉及范围外公共文件时先由主会话协调。无需重复审计已明确验证的同一缺陷；没有确定缺陷时交付真实审计和剩余条件，不为产生 diff 另建状态机。

构建由主会话统一执行，复用 `/tmp/agentkit-layout-text-292-target` 和既有精确依赖，不创建每个子会话独立 target。先执行各子项针对测试，再根据公共接口变更运行共享包、宿主和独立 consumer；验证证据区分静态、组件、协议、离屏与原生。不得把构造通过当作业务验收。源码、WIP、固定依赖、程序与验收证据保留。

## 暂不并行写入

- #17 Transcript 与 #21 RunPort 共享 composition/reading/布局链，待上述批次稳定后再划互斥范围；可做只读核对，暂不增加写入者。
- #16 Composer 本轮已完成非 SDK 定向核对；宿主附件适配与布局/焦点等须有明确实现路径再分配，系统 IME 不在本轮。
- #19 真实 report/group narration producer 仍未确认，不能猜造协议 fixture；明确来源后再实施。
- #18 的 #53/#42、#20 的 #47/#49、#26 的 #48，以及 #25 中相应 SDK 视觉子项，按具体公开能力等待；不是整个 Issue 都阻塞。
- #24 Native/Pi 非桌面闭环已有分项证据；当前新增剩余以原生生命周期和 DSH 公开能力为主，不重复造实现。
- #27 原生窗口、系统 IME、VoiceOver 与 ProcessContainer 验收暂不执行；#28 合并/tag 在完整门槛未满足前不执行。

## 集成门槛

1. 主会话审查实际 diff、源身份和针对测试；无依据的行为变化退回。
2. 公共 API 变化同步文档与第二 consumer；新聚合不得弱化单端来源校验，不改 SDK 或复制搜索/输入状态机。
3. 结果回填各自 Issue：实现提交、已验证、未验证、PR；设计变更同步设计与 Issue。原生待验收和 SDK 阻塞保留，不能凭测试计数关闭宽范围 Issue。
4. 统一提交/推送后检查新 head CI；严格 #53 失败如实保留，不豁免、不合并。

桌面验收仍暂停；只消费公开精确 TekesUI，不改 Swift、Kernel 或 TekesUI。

## 本批交回与审查

三个子会话均已交回产物，由主会话完成审查、必要修正和集中验证；实际提交、测试、独立 consumer、Pi/DSH 非桌面补验以及剩余工作见[本批统一收据](rust-agent-ui-parallel-results-20261024.md)。分工表不代表 Issue 已关闭；后续可继续复用这三个子会话，各自文件范围不交叉。

## Sidebar 宿主接入续批（ddc49c7 基线）

继续复用以上三个独立子会话：#22 改 demo host adapter、`component_demo.rs`、`components_native.rs` 及专属测试；#23 只新增 SidePanel/child 回归与专属审计；#25 修改 presentation Summary 校验及专属回归/审计。主会话拥有 Sidebar pending 退休公开 API、consumer 生成器、共享文档/索引、集中构建、审查与 Issue 回填。构建仍串行使用原共享 target；桌面验收暂停。新宿主接入不代表跨端搜索或真实三 harness 同窗已验收。
