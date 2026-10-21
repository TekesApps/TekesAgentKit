# Composer 输入胶囊与原库焦点路由（实验）

在原输入行布局上增加真实 `input_band` 节点，包含同一份 TekesUI 编辑器及 Send/Stop、附件按钮；context 卡片和 model/preset setup 仍在胶囊外。对照当前 ClientKit `ComposerCardView.layout/fittingHeight`、`ComposerInputEditorSurface` 和 Tekes `ComposerDesignAcceptanceWindow`，仅修改 Rust AgentKit。原库 SurfaceView、编辑器、按钮、字体与 token 保留，没有复制命中、焦点、IME 或绘制引擎。

## 宿主接入

`ComposerIds::new(base)` 现在保留 **9 个连续 ID**，新增 `input_band = base + 8`；原 8 个 ID 不变。宿主必须扩大原保留范围，`RunView` 的 namespace/header 冲突检查已包含新节点。胶囊内编辑器与原主动作随会话、运行状态切换保持原有 owner；setup 控件不进入胶囊。

默认公开 TekesUI pin 仍为 `8cf0840790334ba224afc0595897c74231c712e6`，没有公开留白焦点和新包装绘制 hook，因此默认不启用这两项候选能力，也没有升级发布依赖。

显式隔离候选中，宿主在原 editor 挂载后调用 SDK 自己的 `set_padding_focus(ids.input_band, Some(ids.editor))`。声明绑定原 editor 的 mount；目标替换或卸载/重挂后必须重新声明，移除声明使用 `None`。直接编辑器和真实按钮继续走原事件链，不把按钮矩形归类为装饰留白。

候选接收探针直接将事件交给 ContentHost，证明实际 Composer 子树可以被原库绑定；**没有绕过正式宿主的 IME 屏障来宣称原生联调完成**。`ComponentDemo` 在组字时需先判断指针事件是否仍指向同一个编辑器，当前公开 SDK 没有相应只读查询。已提交 [TekesUI #34](https://github.com/TekesApps/TekesUI/issues/34)：查询必须复用原库的 overlay/modal/capture、可见性、后代优先级及 mount/可用性判定且无副作用。补齐前保留屏障，不在 Kit 复制判断逻辑。

## 固定候选与证据边界

接收侧校验输入 `95b140c278a2257c11fb956eaf860313c38bc4829bae70e022cc3d8f33bb38ac`，496 文件；归档 SHA256 `fb1abf3524b5aa44c585a323402779cd6ac8adc7e5578b59dbc9889773dbf523`。所有包来自冻结解压源，不消费 TekesUI 活跃 worktree。接收脚本保存源码快照、显式改写清单、文件哈希及检查收据；复用现有 target，未创建独立编译缓存。

接收侧专项检查：真实胶囊留白聚焦原 editor、已有组字与 selection1..3 保持、真实发送动作优先、禁用按钮不被留白绑定截获、声明移除及 setup 不在绑定范围。原 Run/rail 事务回归继续执行。

同一候选的实际 `TurnRailAction` 仅将 `focus_decoration_above_siblings` 委托给持有的原 CompoundButton。测试确认键盘焦点、禁用及非键盘焦点状态从原按钮传播；未复制原按钮条件，没有自行绘制 ring，也没有重新排序内容树。这是包装协议验证，不能作为原生焦点环像素、完整 GPU 或 VoiceOver 验收。

本阶段尚未完成 #34 对正式宿主 IME 策略的接入、候选真实窗口留白焦点、完整物理输入法和 ring 视觉验收；也不替代三端 UI、分页/阅读、完整 ClientKit 同名 Views 的交付要求。验证终态与日志身份见同名 JSON。

## 原生回归发现的测试宿主布局问题

未带 `--transcript-content` 时，主 demo Stack 仍让 diagnostic 文字占剩余高度，而其下 RunView 本身也请求完整 viewport，导致 Composer 被放到窗口外。修复测试宿主的 flexible child 为当前 RunView，并在切换会话时更新该 ID。没有改变共享 RunView 状态机。新增回归覆盖520/760高度、A/B/A切换及原 editor/主动作/胶囊始终在窗口内。保留失败二进制、日志与初始截图，修复后的原生结果独立记录，不回写失败收据。

## 本轮检查

默认公开 pin 全包351项、固定95b140候选全包371项测试通过，两条路径全目标 strict Clippy通过。专项19项包含不变的原 Run/rail/Start探针和本轮留白测试；实际包装的1项单元测试及第二公开consumer35项均属于整包子集，不累加。fmt、Rust UI边界和默认实际解析的8包/199文件校验通过。默认原生fixture修复后通过输入、流式、取消、重连、草稿以及任务/子代理/SideChat隔离，未将其硬编码范围外的reading-anchor/physical-ime记为通过。初始50ms截图有一次白帧，仅作为诊断保留；已查看后续SideChat截图，确认两个Composer位于窗口内。

缓存沿用 `/tmp/agentkit-layout-text-292-target`，debug0、incremental关闭；阶段结束不新建target。该共用目录后续验证仍需使用，本轮没有认定可安全删除的额外缓存，删除0字节。保留全部源码、候选闭包、失败/成功日志和独立可执行文件及截图。

同一默认二进制开启 `--transcript-content` 后的原生组合也通过；实际截图可见历史、两份Composer、用户气泡/footnote居右和机器正文/footnote居左。本项仍不证明真实三端分页阅读锚点、物理IME或候选留白门禁通过。两次成功窗口均已退出；TekesUI #34 的后续固定输入另行验收。
