# Summary 标题槽与 #31 候选消费

本轮补齐同一 Summary header 的库组件组合，继续对照 ClientKit `ClientAgentSummaryView.swift::SummarySectionHeaderButton`。Tekes 的 `ClientAgentSummaryHostAdapter.swift` 只提供 repository sections 与导航回调，未将业务 header 或折叠状态移回宿主。

## 默认公开版本中的变化

`SummaryHeader` 将 TekesUI 已有的 `CompoundButton` 与一个被动 `TextContent` 标题槽组合。标题与 header 分别分配受同一 scope 管理的身份，更新保留原控件；完整标题同时交给按钮的 accessible name 与原文本模型。文本槽不成为第二个 AX 元素或焦点目标。折叠偏好、expanded 语义、更新期间的原控件保留及旧动作拒绝继续由现有 AgentKit Summary 实现。

按钮的输入、命中、焦点、绘制、子主题和布局全部委托 TekesUI；没有复制按钮、文本编辑、文字排版或通用 disclosure 引擎。颜色、字体、圆角和交互反馈使用库默认值。此组合为显式标题策略留出公开内容槽，**默认公开 pin 的单行省略能力仍缺失**；没有在消费侧预截断字符串或强压行高。

默认 pin 仍是公开的 `8cf0840790334ba224afc0595897c74231c712e6`，8 个库包、199 个已审输入。不会因为接收到开发候选而静默改 pin。

## 精确开发候选

接收方验证归档 SHA-256 `6d00adbd7a975823ad772a2fef22d925e4756f4c28c3dc036115abb5d3313630`、476 个文件及源码身份 `545ef8d70d89404f11d916f05e417404277507b07dea808aac051339544431a1`。它基于原 2b16 候选；不混入 Gallery 后来的集合、Tooltip 等 WIP，也不借用另一个 Root 身份的原生收据。

`test-reading-candidate.py --summary-labels` 在新建的 AgentKit 副本中显式做以下适配，并记录各个实际覆盖文件的 hash：

- 既有 Summary header 的装饰 `TextContent` 槽，以及真实保留的 TextContent/ButtonContent 行，消费 `with_label_layout(LabelOptions::single_line(TailEllipsis))`。
- 组件包增加对同一候选 text 包的直接依赖；默认清单和 lock 不变。
- 既有 native `TextBackend` 向原 CoreTextShaper 转交 `shape_label`，复用当前字体、字号和 glyph renderer。AX required 字段从公开语义转交。
- owned native fixture 提供长中文、英文、emoji 和组合字符标题；原 Summary 折叠探针额外核对完整 AX 名称和单行边界，并可截图。没有修改候选本体或已安装 app。

重放命令：

```sh
PYTHONDONTWRITEBYTECODE=1 python3 scripts/test-reading-candidate.py \
  --source <545-source> --manifest <portable-manifest.json> \
  --probe scripts/probes/agentkit-summary-label-candidate.rs --summary-labels \
  --workdir <new-agentkit-replay-directory> --target-dir <owned-build-cache>
```

需要 Python 3.11+、Rust 1.95.0 和 macOS/CoreText。脚本不提升冻结包，不更新默认清单。原有 `--probe` / Transcript-only 路径保持可用。

## 验证与边界

[同名 JSON 收据](rust-agent-ui-summary-label-slot-20261021.json)将默认公开 pin 与开发候选的构建、测试、原生窗口、图像和源码 hash 分别记录。候选的 54 组 receiver 组合覆盖三种长文本、三种宽度、三档字号及浅/深主题；真实 header 与行都请求库的单行排版，原始模型和单一完整 AX name 保持，更新不改动作目的地，折叠移除不可见行。缺少 label service 时明确报错，默认正文仍自然多行。另重跑四项 Transcript 事务及八项 RunView 事务，不用“标签通过”替代阅读恢复验证。

这些测量/组件测试不独立证明省略点的像素质量、全部字素边界、物理 IME、VoiceOver 或 paired ClientKit/Gold7 视觉等价。原生窗口验证仍是 owned fixture，不能推成真实三 harness 的索引或阅读锚点验收。

[TekesUI #31](https://github.com/TekesApps/TekesUI/issues/31) 仍开放。当前候选尚无本轮验证对应的公开不可变版本，不替换默认 pin、不自动建议完整验收通过。完整 header divider/hover chevron、Summary 默认行分栏、自定义 repository row 插槽，以及其它 Views 的严格布局清单仍开放。
