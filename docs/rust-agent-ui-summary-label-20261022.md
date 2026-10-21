# Summary section header 的公开单行标题契约

当前精确公开 TekesUI `09b4bab44b6dfa65487250ea816b610b9df59095` 已包含 `Content::with_label_layout`、`TextSurfaceHost::shape_label` 和 CoreText 的 `shape_label`。此前“默认依赖尚未发布 #31 能力”的判断已过时；此次保持 SDK pin 与完整输入清单，不消费兄弟工作树或隔离候选。

只读对照 ClientKit `Sources/Composer/Agent/ClientAgentSummaryView.swift` 的 `SummarySectionHeaderButton`：标题 `maximumNumberOfLines = 1`、`byTruncatingTail`，被动 label 不单独进入 AX，原 header button 保留完整名称和折叠动作。Tekes 的 `ClientAgentSummaryHostAdapter.swift` 仍负责产品环境及仓库能力。普通 `SummaryDefaultRowView` 标题允许换行，本次不将 header 策略施加到普通行或用量行。

## 宿主接入

AgentKit 在原 CompoundButton 的原 TextContent 槽上声明 `LabelOptions::single_line(LabelOverflow::TailEllipsis)`。完整业务标题留在 TextContent.text 和原按钮 AX name 中；仅可见 glyph 由 SDK 省略。`SummaryView::section_header_label_id(section)` 提供原装饰槽查询，`section_header_id` 继续用于原折叠动作。发布相同 owner 的新 revision 不重建这两个控件。

宿主必须通过 `ButtonTextRenderer::text_surface_host()` 提供 `TextSurfaceHost`，并实现其 `shape_label(text, role, width, options)`。macOS demo 的 `label_host` 仅将参数转交当前固定 SDK 的 `CoreTextShaper::shape_label`，使用同一字体 registry、TextRole 字体/weight、type_scale 和同一绘制路径；不手工截字符串、裁字符、估算宽度或换编辑器。components 只依赖纯 SDK label 类型，CoreText 适配留在宿主。

缺少 label 服务时 SDK 明确返回 `CompositionError::MissingLabelServices(label_id)`。这也是渲染 Summary sections 的新宿主要求，不以回退换行悄悄改变源布局。现有实验 UI 版本仍为0.0.0/publish=false。

## 验证范围

第二公开 consumer 自己实现 SDK TextSurfaceHost，没有依赖 demo 或其 label_host helper，验证长中文、英文路径、emoji、组合字符；三种宽度、三档字号、浅/深主题下保持一行、真实省略区间、完整源文本、唯一完整 AX name 和原 owner/ID；缺服务有明确错误。此处为 CoreText 组件证据，不能替代原生窗口或实体 IME。

原生入口：

```sh
cargo +1.95.0 run --manifest-path ui/Cargo.toml --locked --offline \
  -p tekes-agent-ui-demo -- --components --transcript-content --summary-label-native-selftest
```

验收需使用现有共享 `CARGO_TARGET_DIR`，不为此命令创建独立 target。探针只使用 owned fixture，与 endpoint-config/offscreen/其它 selftest 互斥；不强制激活窗口。验证原 Summary 标题 SDK 布局、完整 AX、同一 header/label 实例、字号及原生窗口 resize、原 Composer focus/中文 marked baseline，并通过原 header pointer 折叠和展开。预编辑与 resize 为程序注入，不代表实体键盘、候选窗、VoiceOver或物理拖拽。

最终 UI workspace 466项、脚本31项、严格 all-target Clippy、fmt、SDK闭包与边界检查通过。同一最终程序 SHA256 `7d9ba96f34ebe5f5bde5e7b6d7cd0f0c80f7faafd067a28f162a8e16b18e2f0d` 通过标题专项、旧 Run/工作区/自定义行 Summary、20→44分页与阅读原生回归，以及系统简体拼音组字→确认→单次发送。输入源前后相同；真实三 harness 的旧证据不归属此次程序。

完整回归曾发现共享 helper 的标题策略影响 Sidebar；已改为只有 Summary 显式启用，原 Sidebar 四项菜单回归和完整 workspace 均通过。初次编译探针遗漏 TextContent import 已补齐。相关失败日志保留，不放宽缺服务断言。

详细程序/源码哈希、测试及原生收据见同名 JSON 和 `.runtime-fixtures/summary-label-20261022/`；历史候选/失败日志保留，不能作为最终程序通过证据。

## 尚未完成

这只补齐真实的标题排版差异，不代表完整 Summary 或其它 Views 已对齐。header hover chevron/divider 的完整显示规则、普通行 leading status icon / trailing accessory、子代理 child-session 状态来源与配对视觉仍需推进。宿主自定义 Content 行已有独立接入契约，不代表所有 repository 产品行已组合完成。

被动图标/不定进度附件缺少原 Content 渲染入口，已提交 [TekesUI #47](https://github.com/TekesApps/TekesUI/issues/47)，不使用 Unicode 图标或消费侧自绘 spinner 代替。原 [#31](https://github.com/TekesApps/TekesUI/issues/31) 可消费 API 已存在；其 issue 开放状态本身不是阻塞理由。主目标继续进行，PR #10 保持实验草稿，不据此合并或打稳定 tag。

复用原共享 target，debug0、incremental关闭；没有新建独立缓存或删除用途不明确的构建产物，本阶段释放空间0。保留固定依赖、源码、日志、收据与最终验收程序。
