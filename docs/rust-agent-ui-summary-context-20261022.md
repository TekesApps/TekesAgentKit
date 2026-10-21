# Summary 的显式工作区上下文

只读核对当前 ClientKit `ClientAgentSummaryView` / `SummaryProviders.SummaryContext` 和 Tekes
`ClientAgentSummaryHostAdapter`：Summary 的 thread 可为 nil，宿主 applicationSections 与默认providers组合；
`SummaryPanelView.summaryRowsBelong` 在没有当前thread时拒绝会话mirror行。应用工作区内容不应通过虚构session获得显示owner。

Rust原SummaryView仅接受RunScope。本轮新增 `SummaryOwner` / `SummaryContextInput` 和
`SummaryView::update_context`，复用原组件、section/文本/用量行、折叠与SDK滚动实现。
原 `SummaryInput` / `update` 保持签名，通过同一实现适配；不修改已发布headless v0.3.8的契约或runtime。

## 宿主输入

```rust,ignore
let mut summary = SummaryView::new(namespace, labels);
summary.update_context(&SummaryContextInput {
    revision: display_revision,
    owner: SummaryOwner::Workspace { id: stable_host_workspace_key },
    facts: None,
    host_sections: application_sections,
    ready: application_snapshot_ready,
})?;
let host = summary.into_host()?;
```

此组件可独立挂载，不需要RunController、SessionRuntime或RunScope。workspace key由宿主提供，需为非空、无NUL、
在该宿主中具有明确命名空间的稳定身份；不从路径、当前选中会话或连接状态猜测。
宿主继续通过原SummaryLabels的section/row格式化回调提供文案；库样式、字体与控件跟随当前精确TekesUI。

- `SummaryOwner::Run(scope)` 可接收完全匹配且验证通过的SummaryFacts和既有会话动作。
- `SummaryOwner::Workspace { id }` 接收显式host sections；facts必须为None，既有OpenTask/OpenSubagent/OpenArtifact动作均拒绝。
  不把另一个会话的事实或artifact权限移给工作区。workspace自定义交互行仍需后续显式host slot/callback，未伪造会话动作替代。
- ready仅在没有任何section时区分“暂无 Summary 内容”和“Summary 暂不可用”；已提供的host rows仍显示。
  它不是连接、提交或动作权限，空数据也不等于已确认的零用量。
- revision是整个组件的单调显示publication，覆盖owner切换及两个update API；不能将不同controller/工作区的局部时钟直接混用。
  相同版本只允许相同完整输入。验证失败保持当前输入/控件/版本，不自动清空或重试。

`displayed_context_input()` 是所有输入的统一查询。旧 `displayed_input()` 仅返回最近通过旧update发布的Run输入；
调用update_context后为None（包括显式Run owner），避免把旧Run输入冒充当前工作区来源。

同owner、同section/row/action身份的原控件保持；owner改变退休旧ID，不会在A→B→A时复用旧动作ID。
折叠仍按section ID本地保留，与ClientKit collapsedSections一致；这是原显示状态，不创建第二套业务presentation。
已排出的typed会话intent仍带原scope，宿主在原binding/adapter上检查其异步权限；UI不撤销或重定向已派发操作。

## 验收

两项新增组件测试覆盖无runtime的工作区host rows/可选中文、同owner控件/选区/折叠保留、owner变化退休旧ID、
空/不可用区分、旧Run动作失效、workspace注入session facts/actions的原子拒绝、冲突版本、旧Run适配器及返回Run不复用旧动作。
第二public API consumer仅依赖公开UI接口，新增 `publish_summary_context` 与无会话authority的独立用例。

原生owned fixture入口：`--components --transcript-content --summary-context-native-selftest`。
原SidePanel中安装同一个SummaryView，显式发布workspace A、折叠、同owner刷新、切workspace B、原header点击展开、空与不可用。
核对规范输入没有RunScope/facts、原行保留/退休及主会话草稿/Transcript/submission数量不变。
这是原生窗口与原控件的合成鼠标证据；整窗仍有独立主Run，不等于完整未选中会话宿主布局验收。

同一最终程序另运行原 `--summary-sections-selftest`：Run路径的折叠/回传、原用量列、未知成本、只读文本与AX选区保留通过。
不声称物理输入、VoiceOver、system clipboard或ClientKit等内容配对视觉已验收。

全UI workspace 459项、全target严格Clippy、fmt、Rust UI边界以及实际Cargo闭包检查通过；
仍为公开精确SDK `09b4bab44b6dfa65487250ea816b610b9df59095`，8包/241输入，无兄弟工作树覆盖。
源码、原生程序与收据hash见同名JSON。UI仍0.0.0/publish=false；完整Summary默认row布局、repository自定义行、
状态pane与所有Views对齐、review/CI/合并/tag仍按总目标继续。

本项没有新增SDK基础能力缺口。复用 `/tmp/agentkit-layout-text-292-target`，debug0/incremental关闭，未新建target；
保留源码、WIP、固定依赖、验收程序和日志，本阶段释放空间0。
