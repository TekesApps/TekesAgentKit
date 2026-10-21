# TekesUI #22/#28 公开依赖与原生 reveal 复核

2026-10-22 重新查询公开仓库：main、HEAD 和 annotated tag `remediation-toolbar-checkpoint-20261021` 的 peeled commit 均为 `09b4bab44b6dfa65487250ea816b610b9df59095`。AgentKit 的 UI Cargo.toml 已精确固定此版本，因此本轮不产生重复依赖升级。#22/#28 仍 OPEN，但所需公开 API 已交付；issue 状态不能代替源码及消费验证。

实际 Cargo 解析的8个 TekesUI 包、241个固定输入重新校验通过，清单摘要为 `44b3174022584d3c099a45b86892d17dbf948905476c0f038be104df12aefe46`。没有使用活动工作树覆盖，也没有修改 TekesUI。

## 接通状态

原生宿主已经消费 `reveal_sidebar_selection` 和 `reveal_run_rail_follow`；共享 Run 阅读发布使用原 SDK `update_scroll_reading_document`。普通回合使用纵轴 Start，最后回合使用 follow_latest。业务层负责来源、版本、挂载及一次性请求校验，SDK 负责布局、阅读锚点和滚动几何。没有通过移动 Composer 焦点或自行计算 offset 实现 reveal。

Sidebar 当前仍是显式导航 attachment；不能把选中行自动 reveal 解释为完整库存列表的连续分页阅读事务。销毁后重建文档的持久阅读仍跟踪 TekesUI #42。本轮没有发现需要另提的新基础组件缺口。

## 本轮原生复测

以下四项均在同一个既有程序执行：`.runtime-fixtures/AgentKitSummaryIndentVerified.app`，SHA256 `2e3287042217a395c37c66987ecab4af4e20cab453edd5ff6d731cf5302d97b9`。程序包含当前 Summary WIP，因此结果属于当前工作区，不能视为已推送 PR HEAD 的新 CI 结果。

- Transcript 原生分页20→44；前插锚点、向上阅读、A/B草稿与阅读位置、重连、Start/Latest 导航通过，原编辑器 owner、焦点、选区和中文预编辑 baseline 保持，无提交。
- 只读历史分页38→75；原分页/刷新控件、前插与刷新锚点保持，主 Composer 草稿保持。
- Sidebar 显式自动 reveal、选中行补读、恢复/失败保留以及原焦点和 marked baseline 通过，无提交。
- 系统简体拼音：仅向独立 demo PID 发送离散键事件，组字/提交流程通过；前后输入源均为 `com.apple.inputmethod.SCIM.ITABC`，未改输入法设置。未以合成 Preedit API 代替此项。

前三项是原生窗口加 owned fixture 的验收，涉及预编辑的部分由探针合成；第四项为系统软件输入法证据。没有新增真实 Native/Pi/DSH、实体键盘、候选窗、VoiceOver 或整体配对视觉验收。此前阅读探针失去窗口激活以及独立 footnote 探针失去焦点的失败日志保留，没有降低断言或改写为通过；本次阅读专项在同一程序独立复测通过。

机器可读收据见[同名 JSON](rust-agent-ui-published-reveal-recheck-20261022.json)，原始日志位于 `.runtime-fixtures/published-reveal-recheck-20261022/`。本轮未重新编译、未新建 target、未删除缓存，释放空间0；保留全部原有源码和 WIP。UI 继续为实验状态。
