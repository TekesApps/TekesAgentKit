# Composer notice 接入

对照当前 ClientKit `Sources/Composer/ComposerStackView.swift` 的 ComposerNoticeConfiguration / ComposerNoticeView：
notice 在圆角输入卡片外上方，缺省完全隐藏且无间隙；存在时与卡片间隔 Snug，左右 Relaxed，
换行 caption 与右侧原刷新控件间隔 Regular、纵向居中。canRefresh=false 隐藏刷新；
refreshing=true 或 enabled=false 禁用。Tekes `ComposerDesignAcceptanceWindow` 通过 configuration
注入 notice / noticeIsEnabled，未提供时为 nil；Rust 宿主沿用显式输入/回调边界。

Rust `ComposerNotice` 只组合 TekesUI TextContent、Quiet ButtonContent、Content 与公开布局/token。
基础字体、颜色、圆角及按钮样式跟随 SDK；默认刷新为可本地化文字按钮，不声称与旧 App 的图标或像素一致。
未引入通用输入、焦点或滚动实现，也未修改 SDK。

## 公共输入与回调

在 Composer 发布当前 RunSnapshot 后安装独立 namespace；该 namespace 不能与 Composer 九个保留 ID
或 ComposerContext 重叠。重复安装会报错。原编辑器、undo、草稿 authority 与 action IDs 不变。

```rust,ignore
composer.install_notice(notice_namespace, ComposerNoticeLabels::default())?;
composer.update_notice(&ComposerNoticeInput {
    revision: next_display_revision, // 本组件生命周期严格递增，重连/切换目的地也不重置
    connection_epoch: snapshot.connection_epoch, // 与下列来源同一次快照
    destination: snapshot.selection.as_ref().unwrap().destination.clone(),
    notice: Some(ComposerNoticeState {
        text: host_notice_text,
        can_refresh: true,
        refreshing: false,
    }),
    enabled: host_refresh_available,
})?;
```

None 不占高度/AX 子树；Some 的空文字仍保留配置行。`ComposerLayoutGeometry.notice` 提供实际几何。
挂在 Composer 内时必须使用 `Composer::update_notice` 校验目的地和 connection_epoch；
独立组件的 `update` 只校验输入格式与显示版本，独立宿主负责其来源 authority。
相同版本只允许相同输入；旧版本/冲突版本拒绝且不改现有 notice。语义树顺序为 notice → context → input。
归档、切换目的地、连接epoch变化及不能接受的新编辑帧均撤销旧 notice，保留原刷新控件实例；
恢复后要发布更新版本的新输入，旧事件不能因相同数值 ID 或 A→B→A 而重新获得权限。

处理原控件事件后、发布新 notice 输入前，使用 `notice_mut().take_refreshes()` 取得
`ComposerNoticeRefresh`，再由同一原组件 `claim_refresh(&request)` 一次认领。
request.source() 是事件时不可变的 destination/epoch/revision/notice 事实；clone 共享一次性认领状态。
已退休来源和重复认领拒绝。第二独立 consumer 的 `composer_notice_refreshes` 示范此流程。
宿主随后在既有刷新服务/控制器上派发，及时发布 refreshing=true；完成时由既有请求身份/epoch
校验回传，发布新的 display revision。UI claim 只是事件 admission，不赋予 endpoint 权限，
不替代异步回传 fencing，不自动重试，不将网络错误转成成功。

## IME 与阅读

原刷新 Content 使用 SDK preserving_focus；宿主若已有组字期间外部按钮屏障，只能通过
`is_nonfocusing_notice_refresh_target` / `is_nonfocusing_notice_refresh_at` 查询当前可见且启用的原刷新入口。
隐藏、busy、异目的地、异epoch或未成功布局的 notice 不放行。不要将所有按钮加入豁免。
正常 Send/会话切换屏障继续由现有 Agent 策略执行；组字文本、选区、focus 与控件身份由 SDK 保留。

notice 改变高度沿用原 RunView 几何和 SDK reading 事务；不额外维护阅读状态，不用像素 delta 补偿。
原生专项只对 owned local SEP 开放：
`--components --transcript-content --composer-notice-selftest`，与 endpoint/offscreen/其它专项互斥。
刷新回调仅由探针计数，不等于真实 endpoint 刷新成功。

## 验收与限制

机器记录见同名 JSON。组件覆盖换行/三档字号/浅深色/三档宽度、隐藏零高度、busy、
旧版本冲突、事件时来源及clone一次认领、重连epoch不复用旧请求、错误目的地、归档与原编辑器保留。
CoreText Run回归覆盖向上阅读时长notice出现/消失，原锚点、焦点与marked baseline保持。
第二公开consumer独立认领原控件回调，不依赖demo内部状态。

原生失败记录保留：首个程序的组字屏障未放行刷新，停在phase3；随后只增加当前原刷新入口的业务查询，
保持其它屏障。最终程序验证卡片外位置、一次回调、busy禁用、隐藏无间隙、原编辑器/focus/中文marked及无提交。
合成原生文本服务证据与系统输入法/物理键盘证据分开；最终程序另执行正式分页/导航回归。

此项完成可选notice公共槽及显式回调，不代表完整Composer palette/chip/model toolbar、
所有ClientKit Views布局、VoiceOver、三端真实notice刷新或完整UI目标已验收。
无新基础组件缺口，不重复创建已有issue。UI保持0.0.0/publish=false，PR10继续draft。

构建复用 `/tmp/agentkit-layout-text-292-target`，debug=0、incremental关闭，没有新target。
源码、固定输入、失败/成功日志与验收程序保留；本阶段缓存删除0字节。
