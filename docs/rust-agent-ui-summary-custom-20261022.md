# Summary 原 Content 自定义行

对照当前 ClientKit `SummaryRegistry.SummaryItem.customView`、`ClientAgentSummaryView` 按section/item保留自定义view及
`ClientSummaryCustomRow`高度通知；同时核对其repository原生行和Tekes Summary host adapter。
宿主提供Git/路径/系统能力适配与产品行（可组合对应领域Kit的控件），AgentKit拥有Summary的顺序、section、折叠、owner与显示来源校验。

本轮允许宿主安装原TekesUI Content，不复制输入、焦点、菜单、布局或滚动实现。
同owner/section/row/content_key的内容保留；动态高度调用原child.measure并使用当前行宽，随后由原布局/ScrollContent安排。
不读取offset补偿，也不通过重建控件解决尺寸变化。

## 接入

`SummaryContextInput.custom_rows`声明原host_sections中需提供内容的行，每项为
`SummaryCustomRowSpec { section, row, content_key }`。匹配行必须存在且action=None；不能覆盖默认provider行或兼任既有会话动作。
此插槽用于产品能力行，不用于在宿主重写Composer、任务/子代理或其它Kit已有Agent状态机/视图。
没有安装内容时显示该原行由宿主格式化的静态文案，不伪造按钮或空的成功状态。

以下是一次独立挂载的最小例子；content由宿主使用公开基础控件构造：

```rust,ignore
fn mount_host_row(content: Content) -> Result<ContentHost, SummaryCustomRowError> {
    let mut view = SummaryView::new(40000, SummaryLabels::default());
    let input = SummaryContextInput {
        revision: 1,
        owner: SummaryOwner::Workspace { id: "workspace-key".into() },
        facts: None,
        ready: true,
        host_sections: vec![SummarySection {
            id: "host:repository".into(),
            rows: vec![SummaryRow {
                id: "changes".into(), title: "仓库状态".into(),
                value: Default::default(), action: None,
            }],
        }],
        custom_rows: vec![SummaryCustomRowSpec {
            section: "host:repository".into(), row: "changes".into(),
            content_key: "repository-controls-v1".into(),
        }],
    };
    view.update_context(&input).map_err(SummaryCustomRowError::Agent)?;
    let root = view.id();
    let source = view.custom_row_token("host:repository", "changes")
        .ok_or(SummaryCustomRowError::Agent("row not declared"))?;
    let mut host = view.into_host().map_err(SummaryCustomRowError::Composition)?;
    install_summary_custom_row(&mut host, root, &source, content)?;
    Ok(host)
}
```

所有子内容需预留与Summary及其它Content不重叠的namespace，不能只改根ID。
`install_summary_custom_row`通过原SDK `ContentHost::insert`安装一次，完整树ID/交互结构检查和Mounted回调均归SDK；
失败保持原树。重复安装拒绝，不覆盖已存在控件。
后续获得当前token，用 `custom_row_component<T>` / `custom_row_component_mut<T>` 查询/配置原宿主组件，再请求普通host render。
同一content_key对应同一控件类型与业务目标身份；标签/状态变化配置原控件，控制器/目标/解释身份改变则换key或row/owner。
宿主继续用SummaryLabels提供section/row文案，基础样式跟随SDK。

## 发布与生命周期

对于可能移除或替换已安装内容的输入，使用：

```rust,ignore
publish_summary_context_update(&mut host, summary_root, &next_input)?;
```

该接口先完整校验新的显示输入、身份/动作和ID分配，然后把将被移除的原custom roots交给SDK `remove`，
让原内容收到Unmounted并清理其焦点/capture，最后应用同一已校验的Summary图；保留的原内容不卸载。
验证失败不会先卸载旧内容。没有异步让出或产品操作夹在这段结构发布中，不承诺回滚宿主lifecycle内部自行发起的外部副作用。
直接 `SummaryView::update_context` 在需要淘汰已安装custom内容时返回明确错误，避免绕过SDK卸载；
初次挂载、同身份普通刷新和未使用custom行的旧Run适配仍可直接调用。
宿主Component应通过原生命周期/Drop管理其观察任务和产品资源；Kit不执行Git操作、不创建业务请求或自动重试。

## 事件回调

事件边界捕获 `SummaryCustomRowToken` 与控件原不可变value/payload；派发前在当前Summary上调用
`validate_custom_row_action`。不得等回调处理时才按数值ID查找一个新的token，或读取可变selected替代原事件值。
宿主可以在保留的原组件上刷新它用于捕获事件的来源；关闭/重新展开及重新挂载后需取得新token。

Token包含原Summary实例/SDK卸载生命周期、owner、显示revision、section/row/content_key与已确认折叠动作的代数。
关闭后AX/输入子树不可用；close→open不能令旧token重新有效。折叠仍调用原SDK Disclosure，代数仅用于业务事件来源校验。
Token为Send+Sync的不可变来源，可随宿主异步请求携带；UI本身仍在原线程操作。

这只是Summary数据/section来源校验：它不证明窗口或其它pane当前呈现，也不是文件/系统/endpoint权限、一次性请求认领或异步结果authority。
宿主仍须使用其原pane/navigation来源和SDK事件生命周期；结果经原产品controller的请求身份/上下文校验，再发布新输入并配置原行。
已经派发的操作不会因token失效而被撤销，未知结果由原控制器显式对账；不自动重放失效事件。
自定义产品事件不会被转换成SummaryIntent或虚构RunScope。

## 验收与边界

四项组件回归覆盖：原控件保留与事件来源、冲突spec/SDK完整树重复ID的原子拒绝、外来实例/owner/remount拒绝、
close/open ABA、内容key替换、禁止绕过已安装内容退役，以及非法新输入不先卸载/合法退役收到原Unmounted。
第二public API consumer验证原SDK按钮事件和不可变旧来源拒绝，并编译检查token可跨线程携带。

原生owned入口：`--components --transcript-content --summary-custom-native-selftest`。
原SDK按钮三次回调、同owner原控件nonce保留、原文本高度变化、关闭AX隐藏、重新展开的旧来源拒绝和owner切换均通过；
主Run草稿、Transcript和submissions不变。宿主行仅计数，不执行真实Git或其它系统操作。
同一最终程序另回归工作区Summary与原Run/用量/只读文本选择专项。
合成鼠标证据不代表物理输入/VoiceOver、完整repository行或ClientKit配对视觉已验收。

全UI workspace 464项、全target严格Clippy、fmt、Rust UI边界和实际Cargo公开依赖闭包检查通过。
最终收据、源码/程序hash与验证日志见同名JSON。原先直接改子树的中间程序/收据保留，最终验收使用SDK退役接口后的独立程序。
UI仍为0.0.0/publish=false，已发布headless v0.3.8与精确SDK `09b4bab44b6dfa65487250ea816b610b9df59095`不变。
本项无新增基础组件缺口；完整默认行布局、repository具体产品控件、未选中会话宿主布局及总目标仍未完成。

构建复用 `/tmp/agentkit-layout-text-292-target`，debug0/incremental关闭，没有新target。
保留验收必需的程序/源码/固定输入/日志；观察到其它构建进程，未删除不明确的共享缓存，本阶段实际释放空间0。
