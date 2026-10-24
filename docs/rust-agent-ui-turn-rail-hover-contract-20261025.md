# #21：原回合 rail 的悬停反馈

基线 `9969f8f`，ClientKit只读HEAD `c289bc9` 的 TranscriptIndicatorRailView / TranscriptIndicatorInkView 为行为参照，Tekes宿主只读HEAD `8858ccd`。只消费公开TekesUI `50ec49f3446924fdbc66b3cff22cd8cfe150fe7c`，不改基础库/Kernel/Swift，不跑桌面。

原TurnRail当前固定6pt tick，没有hover业务反馈。现有SDK已有原PointerEvent、interactive/pointer_regions、原ContentHost路由以及外部时钟/动画请求。产品层消费它们补齐连续hover位置与邻项ramp：以原已排列行位置解析pointer rowPosition-0.5；距离0/1/2/3/其余长度26/20/14/8/6，以小数smoothstep连续混合。指针移动直接跟随中心，进入/离开才重定向influence；淡入淡出消费公开SDK motion/state transition与宿主时钟/reduced-motion，不自建调度器、时钟、通用动画或滚动引擎。

原CompoundButton继续唯一拥有点击、Press时机、键盘/AX、焦点、capture和navigation票据；hover不consumed、不产生intent、不改变父draft/selection、follow/reveal。pointer regions只使用原已clip的track；scroll坐标来自原ScrollContent/已排列行，不写offset、不另建命中分发。重排/换源、旧epoch、invalid layout、empty或卸载不得把旧hover index解释成新turn；同controller/scope/epoch与相同有序turn IDs的正常流式publication保持hover和fade，原navigation票据仍随publication退休。保留原row/control/tick身份，颜色/主题/基本描画沿SDK tokens。

带数据的原TurnPreview接口不等于自动浮层。自动锚定/越界preview依赖TekesUI #46，selection-action overlay依赖 #25/#44/#46；本切片不旁路这些能力，也不把tick反馈称为完整rail/完整RunPort。

验证使用原ContentHost PointerEvent与真实component树，检查中心/半行/邻项/leave、原scroll后坐标、换源退休、原点击一次且不夺焦点、宿主时间淡入淡出和reduced-motion；可核对Scene几何，但不称桌面视觉/物理输入/AX验收。共享UI组件、demo、独立公开consumer按改动范围串行验证，复用SSD共享target，不新建cache；保留失败、固定输入、源码、程序和日志，#53仍不豁免，不合并/tag/发布。

独立consumer发现的实际布局差异也归本切片：原ScrollContent自动在viewport内预留ControlFrame焦点边距，旧42pt track使真实row仅32pt宽，原16pt leading后最大tick被压到16pt。现在 `for_run_at_scale` 使用公开 `ControlFrame::focus_bounds(actualBackingScale)`，track明确表示外层viewport，document_size明确表示不含SDK边距的tick行。常规/紧凑门槛与位置、少量行的高度预算均计入边距；纵向仍限制pane的70%，空列表保持零track高度。RunPort在原arrange捕获同一scale，初始geometry和publication refresh都用相同入口，不在子组件另外补一份viewport，不写原scroll offset/reading状态。

实际独立consumer复用既有CoreText文本服务，通过原RunPort树测量、指针事件和Scene clip检查1x/2x/0.5x、同host切换scale、普通publication重绑、常规/紧凑门槛与极窄rail；最大26pt真实ink可见、两侧不侵入Transcript。点击保留Composer焦点并仅产一次导航。Scene几何不等同于GPU或窗口像素验收。motion新增直接依赖仅指向同一精确SDK，锁文件只增加既有motion包的依赖边，原8包/243文件闭包不变。
