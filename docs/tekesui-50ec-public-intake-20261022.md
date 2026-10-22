# TekesUI 50ec 公开精确依赖接收（2026-10-22，实验状态）

> 后续当前程序验收见[输出信息与原生复验](rust-agent-ui-output-information-20261022.md)；下文保留纯pin接收阶段的证据边界。

默认实验依赖固定为公开 `50ec49f3446924fdbc66b3cff22cd8cfe150fe7c`，
上游 #51 数字输入/shared-key checkpoint。6 个直接和 2 个传递 SDK 包固定同一完整 Git rev。
Cargo.lock 仅变更 8 个 SDK source 行，registry 版本与连接未变，`--locked --offline` 可复现。
已发布 AgentKit 契约复核仍为 v0.3.8；UI 仍是 0.0.0/publish=false。

## 公开来源和实际差异

匿名读取完整 commit、canonical root tree `a246629a32248111c6d56f7dc4f63762bde56b8f`，
按 Git blob ID 逐一匹配实际 Cargo checkout 的消费闭包；9 个变更文件另经匿名 immutable raw 比对。
实际 8 包/243 文件，清单 SHA256
`3ad8aa98a567679b775e78b634f45888c9eb70ffd05708a4f781c0ce7264471c`。
保留最小固定源码闭包，没有再次下载完整仓库归档，也没有读取活动 sibling worktree 作为依赖。
原 3bef 清单保存在 `ui/baselines/tekesui-3bef66754a256e7723887de7d33518f5122805ad.json`。

消费范围内，新增 NumberFieldContent/格式化/提交结果等数字输入 API；既有 NumberField step
在边界 clamp；TextFieldContent 增 show_metadata（默认 true），TextSurface 增单行对齐
（默认 Leading），共享 caret/hit/selection origin；help 识别 semantic parts 的区域。
共享 key chain 新增无修饰 moveUp/moveDown 的候选确认去重。
本阶段未引入数字业务控件或改 AgentKit 生产 Rust；新能力不自动代表旧缺口已修复。

## 验证和未完成项

上游精确提交 CI run 39740998580 success。
本地 UI workspace 521 passed、1 已知 #49 ignored；严格 all-target Clippy 和 demo build 通过。
独立公开 consumer 的原 Composer 用真实 CoreText 验证：中文候选提交后的同一 key chain
不再次处理上下键，原 editor/focus/完整 committed baseline 保持；随后普通上下键仍移动光标。
这是组件事件链证据，不等于系统拼音、实体输入、候选窗或原生窗口验收。

原 #49 opt-in 门禁单独复跑仍 exit101：expected popup 51002，actual background 51003。
保留失败、原断言及正常 suite 的已知忽略，不宣布 Popover 事件优先级通过。
#48 ProcessContainer 仍 open，本次消费闭包不含 module-runtime/app-protocol，保留原同进程路径。
#47 图标、完整窗级组合、全部 Views 配对视觉，以及新程序分页/焦点/系统拼音均仍待验收。
不借用 3bef 或更早程序的原生成功，PR #10 保持 draft，不合并、不打 tag。

原始 public tree、最小固定源码、metadata、日志和失败在
`.runtime-fixtures/tekesui-50ec-intake-20261022/`，摘要见[机器记录](tekesui-50ec-public-intake-20261022.json)。
复用现有 UI target，未新建 target；固定输入、验收程序与日志保留，本阶段未删缓存。
