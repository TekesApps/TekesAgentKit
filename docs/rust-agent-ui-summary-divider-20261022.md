# Summary repository header 分隔规则

对照当前 ClientKit `ClientAgentSummaryView.reconcile`：在实际 section 顺序中，只有 ID 以 `repository:` 开头、且不是首个显示 section 的 header 带分隔线。折叠只隐藏 body，header 和分隔线继续显示；空 host section 在判断首项之前移除。Tekes `ClientAgentSummaryHostAdapter` 继续提供产品 repository sections、计数和导航能力，没有另一套 header 规则。

AgentKit 现已将这个规则加入共享 `SummaryView`，不是 demo 专用显示分支。宿主继续使用原 `SummaryContextInput.host_sections` / Run 输入适配，不新增产品开关，也不在宿主复制判断。默认 providers 不因位于后方而自动获得 repository 分隔线。

## 组件与公开查询

`SummaryHeader` 保留原 SDK CompoundButton、Disclosure、标题槽、AX 和事件 owner。分隔线绘制委托当前公开 TekesUI 的 `layout_view::SeparatorView`，使用原 palette、clip 和公开默认 render。逻辑高度来自 `stroke-rule`，标题带前预留该高度加一份 Regular；左右沿用标题锚点的两份 Regular。SDK 继续提供线条、颜色、主题和裁剪，AgentKit 仅负责业务组合的位置。

原按钮仍覆盖完整 header，包括分隔线区域；点击该区域走原 SDK 按钮和原 disclosure，不产生第二个动作/焦点/AX 节点。Summary 非 first-responder 策略保留。共用 helper 的 Sidebar kind 不启用此规则，也不增加高度。

`SummarySectionLayout` 新增 `divider: Option<Rect>`，通过原 `section_layouts()` 读取：

- 返回最近实际 arrange 的逻辑 separator band，在祖先裁剪之前；不是可见 ink 的包围盒，也不是可交互 target。
- 首项、非 repository 和尚未完成当前布局时没有对应有效几何；折叠 section 仍可有 divider，此时 body 为 None。
- 换序/更新令旧 layout 失效，下一次原 SDK render 后返回新位置。原 section/header/row ID 在同 owner 下保留；更换 owner 继续按原生命周期淘汰旧 ID。

UI 仍为实验0.0.0/publish=false；手工构造该公开 layout struct 的消费者需要补新增字段。已发布 AgentKit base 仍为 v0.3.8 / `f1daa0ffcf6ced41e21f351e22845455ca78006c`。TekesUI 精确公开 pin 保持 `09b4bab44b6dfa65487250ea816b610b9df59095`（peeled checkpoint tag 与 main 再次核对一致），八包241输入，manifest SHA256 `44b3174022584d3c099a45b86892d17dbf948905476c0f038be104df12aefe46`。

## 验证

- UI workspace 478 tests、脚本31 tests、strict UI all-target Clippy、fmt、边界及实际 Cargo 输入校验通过。完整套件覆盖最终业务/测试/原生源码；未改动的 headless workspace 本轮未另作本地重跑，最终提交CI另验证完整树。
- 第二公开 consumer 覆盖四种主题（含高对比）、三档字号、两档宽度：过滤空项、首个/后续仓库、相似但不带冒号的前缀、原逻辑高度/标题偏移、折叠保留、同 owner 换序与选区、跨 owner 淘汰。实际 Scene 的 separator ink 与原 SDK SeparatorView 独立输出比较，未以硬编码颜色作为验收标准。
- 最终固定程序 `.runtime-fixtures/AgentKitSummaryDivider.app`，SHA256 `4707329834396be1beff4b6490ff41b50ab39d326f74dc5086bbd0fdaac2ede3`：divider、原 context、default rows、label、child status、sections、custom Content、reading、Sidebar restore 九个原生模式通过。
- 新 divider 模式在真实窗口点击实际 separator band，验证原 disclosure；折叠时换序到首位后分隔线消失，新 owner 的正常顺序使其恢复。原 Composer 的焦点、选区/marked opaque baseline、主 draft/Transcript 与零提交断言保持。截图已人工查看，但不是 Tekes 配对视觉验收。
- 同一程序通过分页20→44、Start/Latest、A/B草稿/阅读恢复、重连和系统简体拼音软件组字→确认→单次发送，输入源保持。fixture 原生模式不扩展为真实 Native/Pi/DSH 新验收；实体键盘、候选窗、VoiceOver 和完整产品体验仍未验收。

原生复现仅使用 owned fixture：

```sh
<frozen-demo> --components --transcript-content --summary-divider-native-selftest
```

可选 `AGENT_SUMMARY_DIVIDER_CAPTURE` 必须为新的绝对 PNG 路径。该模式与 endpoint 配置、其它 selftest 和 offscreen 互斥；不修改真实 repository 或用户会话。默认公共组件的分隔规则无需此 flag。

首次 consumer 断言错误地用宿主外宽计算线长。诊断显示 SDK 列表有自身 inset，已改为按实际 header frame 验证两侧锚点；组件逻辑未加入像素补偿。失败日志和诊断值均保留。

机器收据、源码/参考/输入/程序哈希和截图见同名 JSON 与 `.runtime-fixtures/summary-divider-20261022/`。复用 `/tmp/agentkit-layout-text-292-target`，未创建新 target，未删除缓存，释放空间0。

## 仍需完成

本轮仅补 repository divider 的业务规则与库消费。header hover/disclosure 指示继续跟踪 TekesUI #38；leading/status 附件 #47、只读整行留白 #30、metadata runs/富内容 #25 仍开放。默认行缩进、具体 repository 内容、完整 SidePanel tabs/chrome、剩余 Views/真实三端组合与配对视觉仍需继续。PR #10 保持草稿，未合并、未打 tag，整体目标尚未完成。
