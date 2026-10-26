# Summary 标题与普通 metadata

对照当前 ClientKit `SummaryProviders`、`SummaryDefaultRowView.configure/layout` 和 Tekes `ClientAgentSummaryHostAdapter`：任务/子代理的标题与状态附件分离，产物标题是原文件名，宿主变更计数是独立 trailing metadata。AgentKit 原默认 formatter 把 raw value（包括 pending/running）拼到标题，与此行为不符。本次修正显示层，不修改 SummaryFacts、SEP、状态分类或动作权限。

## 公开接入

- `SummaryLabels::row` 继续负责完整标题，默认只返回 `row.title`。不截断源字符串，不把 task/child execution status 追加到标题或冒充 metadata。
- 新 `SummaryLabels::trailing: SummaryTrailingFormatter` 是 `Arc<dyn Fn(&str, &SummaryRow) -> Option<String> + Send + Sync>`。宿主可捕获已有只读产品投影，例如变更计数；Kit 不执行 Git、文件或系统调用。默认内建 tasks/subagents/sources/artifacts/usage 不生成普通 metadata；其它 host section 的非 null value 按原 string/JSON 格式投到 trailing 槽。宿主可返回 None 隐藏，也可显式覆盖产物 metadata。
- 两个 formatter 都在完整 publication preflight 中捕获，不在 measure/paint 或 apply 中重算。它们必须是本次输入的纯投影，不应执行 mutation、IO 或导航。捕获的数据变化时宿主必须提高 display revision。相同输入/revision 下格式化输出变化会在改动原图之前返回 `Summary formatted content changed without publication`；旧文字、metadata、控件和动作保留。
- `SummaryDefaultRowIds` 增加 `trailing`，指向原 SDK TextContent；缺省 metadata 的可见 frame 为零。原 row/title ID 和 `passive_text_id` 语义不变。值变化或 None↔Some 不替换原 title/field/button/metadata owner。原 action 仍只由原按钮发出，不从 metadata 派生第二个动作或编辑 owner。
- usage 仍用既有 `usage_row` 与 `usage_row_ids`；custom host Content 仍用既有插槽，不调用普通 trailing formatter。host counts/tone 不混入 SummaryFacts 重新计算。

```rust
use std::{collections::BTreeMap, sync::Arc};
use tekes_agent_components_ui::SummaryLabels;

// counts 是宿主已有投影；本例捕获不可变快照。
fn summary_labels(counts: BTreeMap<String, String>) -> SummaryLabels {
    SummaryLabels {
        trailing: Arc::new(move |section, row| {
            (section == "artifacts")
                .then(|| counts.get(&row.id).cloned())
                .flatten()
        }),
        ..Default::default()
    }
}
```

真实动态宿主也可捕获自身有版本的读取入口，但必须让一次 publication 使用一致的快照，并为新数据发布新 display revision。该回调不能替代 host projection 的 authority/version。第二 consumer 的例子验证捕获产品计数、同 revision 冲突拒绝以及新 revision 保留原行。

## 布局与验证

普通 metadata 消费原 TextContent、UiCaption 与 `single_line(Clip)`，完整文本保留。Summary 只负责自己的列：左右两份 Regular，metadata 贴右，标题扣除 metadata 和一份 Regular 后按原 SDK 换行；metadata 相对标题第一行居中。行高仍由标题与标准 body 行高决定。字体、主题、字形、标签裁剪和按钮反馈均来自 SDK；没有手画文字、拼接多个假富标签或自建焦点/选择引擎。

本次仍固定公开 TekesUI `09b4bab44b6dfa65487250ea816b610b9df59095`，实际八包241输入，manifest SHA256 `44b3174022584d3c099a45b86892d17dbf948905476c0f038be104df12aefe46`。UI 为实验0.0.0/publish=false，默认显示语义和新增 struct 字段须按此开发分支接入；不是新的已发布 AgentKit 版本。

- UI workspace 477 tests（包含第二公开 consumer）、脚本31 tests、strict all-target Clippy、fmt、边界和 Cargo 实际闭包通过。477项覆盖最终业务与 publication fencing；之后的原生诊断修正另经 strict Clippy/build/native 验证，完整最终树由提交CI复核。未修改的 headless workspace 本轮不另做本地重跑。
- 第二 consumer 覆盖两种主题、四档字号、三档宽度，metadata 有值/移除/长值、首行和右端对齐、原行/按钮/field/slot ID、文字选择和动作焦点；另验证捕获的宿主计数和同 revision 冲突拒绝。默认 provider 不泄漏 raw status 有单独回归。
- 最终固定程序 `.runtime-fixtures/AgentKitSummaryMetadataVerified.app`，SHA256 `f0e9450772de1b2e92cd9ff739acf4106271dd815f5276961e70fbcfbd9ffaa9`：八个原生模式（workspace context、default rows、label、child status、sections、custom Content、reading、Sidebar restore）通过。workspace 模式验证标题/metadata 分离、原 metadata 的首行布局、折叠更新/owner变化、原 Composer marked baseline 与零提交。
- 同一程序保留分页20→44、Start/Latest、A/B草稿与阅读恢复、重连防重发；系统简体拼音软件按键确认组字→确认→单次发送，输入源保持。没有把 fixture 记为新真实三端验收，也不代表实体键盘、候选窗或 VoiceOver。
- 中间程序、编译失败及旧 label 探针失败均保留。原诊断只保存最后一个 label，新增 metadata 后会取到空 metadata；最终使用有界的原 SDK label 收据，按完整源文本查找。保留原单行/尾部省略/AX/焦点/组字断言，没有放宽业务校验。

验收日志、程序/源码/输入哈希、截图和 machine receipt 见同名 JSON 与 `.runtime-fixtures/summary-metadata-20261022/`。复用既有 target，未创建新 target、未删除缓存，释放空间为0。

## 仍需完成

普通 metadata 仅为 plain text，尚无 change-count 的多语义 run/tone；需求已补到 [TekesUI #25](https://github.com/TekesApps/TekesUI/issues/25#issuecomment-6326892262)，不以两份标签或硬编码颜色代替。leading/status 附件继续跟踪 #47，静态文字整行留白路由跟踪 #30。缩进、完整 host repository 行、hover/divider、全部 SidePanel chrome、富 Transcript、真实三端剩余组合和配对视觉仍需继续。PR #10 保持草稿；未合并、未打 tag、未宣告整体目标完成。
