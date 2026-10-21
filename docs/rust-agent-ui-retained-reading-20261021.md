# 保留会话阅读、真实三端分页与重连

本轮修复了一个实际的重连阅读跳动：读者位于已完成回合 footnote 内时，stale 帧把已知分叉控件从目录删除，footer 缩短，SDK 合法地退到下一项。新鲜帧恢复后，原阅读点已经丢失。修复仍在 AgentKit 的业务 presentation 中，不改 TekesUI，不补像素。

## 对照与修复

对照当前 ClientKit 的 `TranscriptTurnFootnote`、`TranscriptScrollCheckpoint` 和 Tekes 的 public-API `MinimalAgentApp/ThreadWindowController`。ClientKit 按已完成边界与已知能力展示回合控件，动作执行另经来源与连接检查；检查点归文档，不跟随窗口当前选中项。

`present_turn_actions` 现在在 stale 时保留能力已知、边界有效、已完成回合的分叉定义，并强制 `enabled=false`、沿用共享未连接原因。即使调用方错误地传 `ready=true`，stale 也不能授予执行权。无边界、运行中回合或无能力仍不产生该动作。原始 ticket、epoch、scope 与派发前检查不变，Web 与原生共用同一目录。

这避免短暂断连移除 footer 控件。没有保留过期可执行回调，也没有在视图中私造 Fork 能力。

## 组件与原生验收

完整核心187项、UI workspace413项（含第二公开consumer42项、阅读专项11项）、26项Python工具测试、两套workspace的all-target严格Clippy、fmt与依赖边界检查通过。

新增 CoreText 回归验证：

- 原 A 的公开分页在 B 显示期间完成，B 文档、草稿、reading token 不变；A→B→A 恢复各自的阅读项及视口相对位置。
- 原组件实例保留；unmount 必须更新编辑授权与 SDK attachment，旧 reading token 拒绝且不安装半帧。
- A 脱离挂载时重连，重新挂载后恢复 A；B 的阅读点与草稿独立。无新增提交。
- 把读点放到 footnote 底部，再发布 stale 和恢复帧；同一 footnote 局部位置误差≤1逻辑点。仅测试输入依测量选择滚轮距离，生产代码没有 scroll_by 补偿。

本地 SEP 原生探针完整经过：分页等待中合成中文 preedit → 页回传 → 同 publication 刷新 → 原生 Unmark → A/B 切换 → 重连 → 再次切换。阅读点、原编辑器、草稿、marked baseline 与零提交断言通过。等待源回执时不忙绘制。

重连验收以**已显示并完成布局的 Run 帧**为准。不能把 binding 刚到的新鲜快照与仍显示的 stale 帧混在一次断言里；探针已补此检查，初始错误及诊断日志保留。

## 真实 Native / Pi / DSH

运行 `scripts/test-real-session-endpoints.py`，显式启用 `--ui-components --ui-transcript-content --ui-history-pages --ui-native-reading` 并传入各自固定 runtime 与已编译 demo。A 的28轮、B 的8轮历史均通过公开创建、prompt 与完成回执生成；provider 为隔离受控服务，无收费调用或用户配置修改。

三端原生窗口都通过公开分页、向上阅读保持、同 publication 刷新、A/B 独立阅读/草稿、相同编辑器 owner 与重连恢复。Pi/Native 的可见内容从50增到56条，DSH从50增到59条。每项读点在原 SDK 布局后按可见文档相对位置检查；没有把 offscreen 回执当成原生证明。

真实端探针在原 public binding 上显式创建分页请求，由正式宿主派发。这样检查的是 Composer 已有焦点下的数据发布；分页按钮的显式激活本身会把焦点移到按钮，不能把它算作发布抢焦点。本地原生探针另验原分页控件与中文 marked。真实端校验文本、选区和原编辑器，合法 authority acknowledgement 或原生 blur 可以使不透明 editing baseline 失效，不要求这种授权 token 等于草稿内容。

收据分别标出 `nativePagingControl`、`syntheticPreedit`、`opaqueBaselinePreserved`、`realHarness`。三端此项 `readingAnchorAcceptance=true` 只覆盖本次分页/保留会话/重连流程；不扩展为所有阅读场景、真实端分页期间物理中文输入、跨进程恢复或 VoiceOver 已通过。流式追加期间持续向上阅读还需要专门的三端原生验收。

## 依赖与剩余范围

TekesUI 仍精确固定公开 `1294987dacabb33c0e411c13ee5ea544a1123f8e` / `remediation-checkpoint-20261021`。没有消费活动 WIP、复制通用 reading/IME 代码或修改外部主工作树。

保留组件树 A/B 恢复不等于销毁后新建组件树。公开 SDK 没有混合 Content 的持久检查点导出/恢复接口：SavedDocument/Anchor 私有，reading result 不含可迁移 item-local 点，运行期 attachment 不能持久化为新挂载操作权。已提交 [TekesUI #42](https://github.com/TekesApps/TekesUI/issues/42)，写明纯值、业务 key 映射、原 token fencing、用户滚动优先、失败与缺失区分及第二 consumer/原生验收要求。

仍需推进只读有界子历史阅读、Sidebar 全库存持续阅读、真实流式阅读场景、其余同名 Views/布局与未完成入口，以及最终 review、合并和阶段 tag。UI 与 PR10 继续保持实验/draft，未标记整体目标完成。

证据位于 `.runtime-fixtures/agentkit-retained-reading-evidence/`：原生逐阶段日志、三端协议/分页收据、失败诊断及按 SHA256 留存的 CoW 可执行文件。全部构建复用原 target，不新建独立缓存；历史证据与用途不明确的旧产物保留。
