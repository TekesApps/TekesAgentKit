# #21：原回合 rail hover 与实际 viewport 几何

基线 `ddcd94a`，实现边界见[契约](rust-agent-ui-turn-rail-hover-contract-20261025.md)，机器收据见[结果](rust-agent-ui-turn-rail-hover-results-20261025.json)。仍为 PR #10 的试验组件，不是完整 UI 对齐或发布验收。

原 CompoundButton 保留 Press、键盘/AX、焦点、capture 与导航；hover 只消费 SDK 原 pointer 路由、实际排列 row frame、公开 motion/state transition 与宿主时钟。连续长度 26/20/14/8/6、180ms cubic ease-out、reduced-motion 与无时钟降级通过；流式 publication 在同源同序 turn 下保留 hover，换源/epoch/重排/卸载退休旧状态。

独立 CoreText consumer 揭示并修复旧布局遗漏：SDK ScrollContent 会预留焦点边距，旧42pt viewport实际只有32pt row，26pt长 tick被压成16pt。现在统一 `for_run_at_scale` 使用公开 ControlFrame/actual backing scale，track 是真实 viewport、document_size 是不含 SDK gutter 的 rows。横向常规/紧凑门槛和纵向70%预算同时考虑边距；原 ScrollContent、reading/reveal 和命中保持唯一所有者。

独立公开 consumer 从真实 row frame 发指针、核对实际 Scene 与 clip：1x/2x/0.5x、同host改变scale、正常publication重绑、常规/紧凑边界、极窄列通过。真实ink达到26pt且不被裁剪、不侵入Transcript；一次点击只产生一次导航且Composer焦点保留。组件另覆盖滚动后的原row坐标、半行ramp、淡入淡出、reduced-motion与退休。

实际验证：UI工作区 **763通过/0失败/1既有忽略**，feature RunPort19、bindings39，默认/feature严格Clippy与两种demo构建通过，独立standalone consumer实际执行通过，Python最终262通过。新增motion直接依赖指向同一公开精确pin；原8包/243文件SDK闭包核对通过，锁文件仅新增依赖边。

初次脚本全量检查发现boundary测试fixture未加入motion依赖，已同步修复；另有一次provenance fixture执行10秒超时，未改生产接口或放宽超时，原测试定向5项与最终全量262项通过。失败日志与初始fixture保留。Rust最终测试不因这一Python输入修正而冒称重跑。

当前程序的严格nested-reading probe仍以exit1、accepted=false失败：parentFollowingAfterInnerUp=true、detailVisibleAfterAppend=false。不豁免TekesUI#53。自动锚定preview仍需#46；选择overlay、桌面/物理IME/OS AX/GPU像素及真实harness hover未验收，不关闭#21，不合并/tag/发布。

证据保存在SSD `.runtime-fixtures/turn-rail-hover-20261025/`：536个source输入、原失败/最终日志、两份程序和SHA、独立consumer来源与锁、原strict JSON。共享target复用，未新增target，删除缓存0字节。新提交CI需按精确head另行核对。
