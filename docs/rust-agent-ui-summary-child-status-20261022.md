# Summary 子代理状态事实的来源

只读对照 ClientKit `SummaryProviders.subagents`：状态点来自关联的 child agent session，而非创建它的工具调用。`running/in_progress/working` 对应 running，`failed/interrupted` 对应 failed，其它状态没有点；完成态不会增加成功点。Tekes 的 `ClientAgentSummaryHostAdapter` 仍只供应宿主能力。

## 公开契约与兼容性

AgentKit 在 v0.3.8 发布基线之上的当前开发分支修正 `SummarySubagent.status` 的含义：它是子会话执行状态，空字符串表示没有观察到该事实。结构字段和 serde 形状不变；外部显式 facts 的生产者也须提供 child 状态。此前把父工具 phase 放进该字段的行为是错误的数据来源，不能继续当作子代理执行态。

- 新 `SummaryFacts::read_with_store(scope, store, transcript)` 读取同一 endpoint 的现有内存投影，拒绝异 endpoint 或不存在的 parent。原 `PresentationStore::refresh` 已改用它，因此现有 RunController/RunBinding/订阅更新可收到子会话状态，无需另一套 UI 状态机。
- 旧 `SummaryFacts::read(scope, parent_session, transcript)` 不具备 child authority，仍提供标题和结果中的原 UUID pointer，但其 child status 保持未知；不会再用工具 `doing/done/error` 填补。工具 phase 继续留在原 Transcript。parent 自身的 UUID 不再作为 child pointer 返回。
- 有明确的 child execution status 时保留原值；inventory 的 `active/archived` 是生命周期值，执行状态复用既有 `thread_activity` 的 child-only 投影，消费 child 自己的 running 标志及已保留的执行事实。没有 child summary 的保留历史不补充 membership/status。不根据父运行、子控制模式、结果文案或 UI 点击猜状态。
- 缓存中的 child status 不保证连接实时。既有 RunSnapshot.connection/epoch 继续表达离线与历史陈旧；断线不凭空将 child 改成成功或失败，也不会自动发模型请求、重试或 hydrate 子历史。
- `SummarySubagent::indicator()` 和公开 `SummarySubagentIndicator::{Running, Failed}` 提供与 ClientKit 相同的纯显示分类；不扩大 pointer、catalog 或可继续运行权限。未知值保留，未识别状态没有点。

UI package 重新导出该 enum。现有 Summary 输入和原 row/control/action 身份保持，状态更新不替换原编辑器或动作目的地。状态缺失时 section row.value 为 null，默认标题不追加假的工具 phase。

## 验证

headless 回归覆盖工具 error 与 child running 相矛盾、child failed/interrupted/completed/未知值、缺失 child、缺失状态及 child running 标志、异 endpoint、parent UUID 拒绝、RunSnapshot 自动更新、断线陈旧标志与零提交。公开第二 consumer 验证 child 状态更新保留原 row 和原 child callback。

协议到控件测试使用独立 owned SEP fixture：父调用已 done，子状态由公开 inventory 完整帧单独更新为 running → failed → completed。同一 RunBinding → 原 SummaryActionRow 收到新事实，原 control 身份、Composer focus/marked baseline 保持，未加载 child journal，没有提交。

原生入口：

```sh
cargo +1.95.0 run --manifest-path ui/Cargo.toml --locked --offline \
  -p tekes-agent-ui-demo -- --components --transcript-content --summary-child-status-selftest
```

该入口仅用于 owned fixture，与 endpoint-config、offscreen 和其它 selftest 互斥。探针通过同一公开 inventory 更新、原 presentation 和真实原生窗口，验证上述事实、原控件/编辑器和合成中文预编辑；不直接修改 UI 状态。精确程序、源码、日志和收据见同名 JSON。

此前协议 fixture 通知漏了 `running/blank/updatedAt`，被原校验拒绝并触发重连；已补齐完整帧，没有放宽校验或延长等待来伪造通过。测试现使用 canonical opaque tool ID，不猜原始 callId 或按数组位置寻找目标；fixture 初始 blank child 的执行态为 notStarted。

## 仍未完成

当前固定 TekesUI 仍为 `09b4bab44b6dfa65487250ea816b610b9df59095`，8包241输入，无 SDK 修改。原生收据明确 `statusDotRendered=false`：本次完成事实来源与分类，现有普通行仍有实验文本状态回退，不代表已经复现 ClientKit 的 status dot、leading icon、trailing accessory、wrapping/first-line layout 或完整配对视觉。图标/不定进度 Content 缺口继续跟踪 TekesUI #47，禁止把文本或手绘图形当成标准附件的完成验收。

本轮的 owned SEP 证据不等于真实 Native/Pi/DSH 子代理状态验收，不改写旧真实 harness 的程序归属。物理键盘、候选窗、VoiceOver、其它 Views 的严格对齐以及 Native lifecycle/archive 仍按整体目标继续推进。UI保持实验状态，PR #10不因本次局部通过合并或打稳定tag。

本地 headless191项、UI471项（含第二公开consumer）、脚本31项及两套strict all-target Clippy、fmt、SDK边界与8包241输入闭包核验通过。最终native-only探针的模型/UI双发布等待另经Clippy/build及原生回归，最终commit仍由CI覆盖完整树。最终程序SHA256：`5726b0583dedbcec91c77d6ab767c06d3c04a13bc1321fcd591453bc464fdcea`，通过子状态回传专项、header单行/焦点/组字、旧Summary选择/用量及20→44分页阅读，并通过系统简体拼音组字→确认→单次发送（输入源未改变）。首个native程序在模型先于UI发布时提前断言，保留为失败中间程序；最终程序等待两者同状态后仍检查原控件/文字和marked baseline。

复用共享target、debug0、incremental关闭；未新建独立构建缓存。其它Rust验证进程和用途不明确的共享产物未清理；固定依赖、源码、失败/最终验收程序和证据保留，本阶段删除0字节。
