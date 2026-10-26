# Summary 用量行：左标题、右数值

当前状态（2026-10-22）：[公开单行标题消费](rust-agent-ui-summary-label-20261022.md)已接通精确 `09b4bab` 的 SDK label 服务。下文“默认 pin 缺少/候选未发布”是历史输入结论，不代表当前阻塞；普通默认行仍自然换行，历史实验不覆盖产品排版。

本轮对应 ClientKit 的 `SummaryProviders.usageItems` 和
`ClientAgentSummaryView.SummaryDefaultRowView`。此前 AgentKit 将 `output · 0`
拼成一个文本控件；现在六个用量行分别挂载左侧标题与右侧数值，标题可换行，
数值靠内容区右边，并与标题首行相邻，不随整段多行标题垂直居中。

## 依赖和所有权

- AgentKit 最新发布契约核查为 v0.3.8，继续复用 `SummaryFacts.sections`；
  不增加用量状态机、计数器、成本估算或端点协议。
- 默认 TekesUI 仍精确固定 `8cf0840790334ba224afc0595897c74231c712e6`。
  不切换为 545/292 等开发候选，不修改上游源码。
- 两个显示槽都是原 TekesUI `TextContent`；布局组合使用公开的
  `ContainerLayout`、`layout_measured_children` 和间距 token。
  标题使用默认 UiBody，数值使用 UiCaption；不复制 ClientKit 的颜色、字体尺寸、
  圆角或绘制代码，TekesUI 的整体样式调整会沿组件继续生效。
- Tekes 当前宿主只注入 Summary 的导航、仓库与文件能力；本轮没有修改其代码。
  `RunSurface` 和 `ComponentDemo` 仍是测试宿主。

## 事实与格式

| 原 facts 行 ID | 默认标题 | 显示规则 |
| --- | --- | --- |
| total | 总计 | 已有 canonical 总量；整数千位分组 |
| input | 输入 | 原 input_tokens，零保留 |
| output | 输出 | 原 output_tokens，零保留 |
| cache | 缓存 | 原 cached_input_tokens，零保留 |
| hit-rate | 命中率 | 原 cache_hit_rate，1 位小数百分比；无分母显示 — |
| cost | 成本 | 仅既有 known_cost_usd，USD 2–6 位小数；不完整或未知显示“未知” |

整数分组不经过浮点转换，可保留 u64 最大值。未知/非法数字不作为零显示。
未来扩展的用量行仍保留原 title/value，不将未知 ID 映射为另一个已知指标。

## 公开接入

`SummaryLabels::usage_row: fn(&SummaryRow) -> SummaryUsageRowText` 是宿主的
本地化入口。默认格式通过 `SummaryUsageRowText::read` 取得，宿主可只替换文案；
不需要重算事实或更改 SummaryInput。该回调用于无动作的 usage 行，其他行继续
使用 `SummaryLabels::row`。使用显式完整 struct literal 的调用方需补字段，
也可以使用 `..SummaryLabels::default()`。

`SummaryView::usage_row_ids(id)` 返回 `SummaryUsageRowIds { row, title, value }`，
用于宿主可访问性/窗口接入与验证。它表示当前模型的控件身份，不能代替成功渲染
及呈现的 frame 回执。正常 publication 和折叠不会重建文本控件；切换 run 时
ID 随 scope 更换。折叠时库的 inactive 子树不暴露 AX 或输入槽，数值行没有业务动作。

独立公开 consumer 示例在 `ui/consumer/src/lib.rs`，无需直接导入 presentation、
端点、平台或 demo 模块。

## 验证范围

测试和原生结果写入同名 JSON 收据。CoreText 布局矩阵覆盖 4 个字号比例、
3 个宽度与明暗主题，共 24 组实际 Summary 内容树布局，检查不重叠、左右边界、
长标题换行、首行旁数值和完整可访问文字。组件用例另查格式、零/未知、
折叠期间更新和原控件身份、新 run 身份隔离。

原生 Summary probe 使用独立本地 SEP fixture 的公开 control projection 提供
2,000 输入、1,000 缓存和 1,234 输出；没有调用付费模型。沿原 binding 进入真实
Summary，执行任务和用量章节的指针折叠/展开、草稿 publication、原控件保留，
检查完整数值 AX 与右侧几何。此证据只证明该原生窗口里的合成事实闭环，
不冒充真实三端用量、物理中文 IME、VoiceOver 或产品视觉对照验收。

## 仍未完成

- ClientKit 的普通 Summary 行标题允许换行，只有 header 标题是单行截断。
  先前 #31 隔离实验对行应用 ellipsis 仅验证基础标签机制，不是可采纳的产品布局；
  默认公开路径继续自然换行，后续候选接入不能把该实验策略带入正式行。
- 全量 Summary 默认行的前置任务图标、子代理状态、产物变更数、自定义仓库行、
  divider/hover chevron 与 workspace 生命周期仍需继续对齐。
- 默认固定库尚无 #31 候选的单行/尾部省略公开能力；极窄列的长数值可能换行或
  裁剪。AgentKit 保留完整文本/AX，不自行实现截断或文字排版。
- 静态行的文本选区仍受既有 TekesUI #25 能力边界约束。
- 完整目标仍包含 Composer、Transcript/阅读与分页、Sidebar、三端验收及最终
  review/CI/合并/tag；本轮不声明整体完成。
