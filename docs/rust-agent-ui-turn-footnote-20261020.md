# Turn footnote：共享动作与原来源回传

基线：AgentKit 最新已发布 v0.3.8（2026-10-20 重新确认），实验 TekesUI 仍固定公开完整 rev `8cf0840790334ba224afc0595897c74231c712e6`、八包199输入闭包。没有修改 Kernel、TekesUI 或 Swift 源码，也没有新增运行时依赖。

## 组件与输入

`TurnFootnote::new(namespace, scope, labels)` 实现公共 Component，可 `into_host()` 独立挂载；同一实例固定 scope / turn identity，以单调 publication 和配套 connection epoch 接收已有 TranscriptTurn。caption、默认 Button、间距、字号和主题全部消费 TekesUI，宿主以 `TurnFootnoteLabels::information` 格式化原有事实。输出 aside 在机器正文外靠左；用户 InputMessage 的独立 footnote 继续靠右。

TurnFootnoteFacts 保留原 status、started_at / ended_at 与 provider 用量：running 使用 live report，其余使用 reported report 和 complete 覆盖标记。不把 legacy usage 默认0当成真实报告，不相加 live/durable，不用文本长度估算 token。时间戳沿用 SEP/Pi 的 Unix **毫秒**，`elapsed_seconds()` 对有效已知区间除以1000；缺失、倒序或非有限区间不显示时长。不补造缺失/裁剪的起点，不使用当前机器时钟猜测运行时长，也不提升原 DTO 的覆盖保证。

原完成回合 Fork 目录此前仅在 Web 层补入；现已移到共享 presentation 的 `present_turn_actions`，Web 与原生 RunSnapshot 共用。会话活跃工作判定也共用 `session_has_active_work`。动作仍保留原 id/label/disabled reason；缺少持久 settlement、stale 历史或无能力时不产生可执行动作。UI 没有私有 Fork 规则。

## 点击与回传

1. 宿主将本次 `ContentHost::drain_actions()` 交给原组件 `take_intents()`。一个点击生成原 scope、turn、boundary、definition、publication/epoch 及共享回执；局部 pending 禁止重复操作。
2. `TurnActionDispatcher::prepare(original_binding, &intent, stable_rpc_id)` 只能认领一次。它确认服务与 binding 是同一 runtime，原 pointer/control role 有效、selection 与连接 epoch 未改变、没有未确认提交，且最新 canonical turn 仍有完全匹配的启用动作与 settlement boundary。
3. 消费 `TurnActionRequest::execute()` 时再次检查上述事实，然后调用已有 `thread.performAction`。SharedAgent 继续在点击时解析精确 boundary 并检查当前策略，所有 endpoint RPC、可选 rename/queue cleanup 和原 outcome 均由现有 dispatcher 处理。
4. 已知完成/派发前拒绝产生新按钮 ID，下一次显式用户点击才可再操作；旧排队动作不可命中新按钮。派发后取消或丢失结果报告 Unknown，保留禁用，不因刷新、重连或新 epoch 自动重试。当前没有 Unknown 自动对账/解锁 API，不能按标题或时间相近来猜测原 RPC 的结果。
5. outcome 的 navigation/warning 原样保留。宿主先核对原导航 token 和 scope；A 的结果在已切到 B 时不得修改 B 的导航、错误或草稿。原组件的回执即使 retained/offscreen 仍有效，重现时 refresh。

示例（省略宿主 renderer / window wake）：

```rust,ignore
let snapshot = original_binding.current_run().ok_or("missing run")?;
let mut footer = TurnFootnote::new(700, original_binding.scope().clone(), labels);
footer.update(original_binding.scope(), snapshot.revision, snapshot.connection_epoch,
              &snapshot.transcript.turns[0], true)?;
let root = footer.id();
let mut host = footer.into_host()?;
let actions = host.drain_actions();
let intents = host.get_mut(root).unwrap().component_mut::<TurnFootnote>().unwrap()
    .take_intents(&actions);
for intent in intents {
    let request = dispatcher.prepare(original_binding.clone(), &intent, host_rpc_id())?;
    let outcome = request.execute().await?;
    // 在原导航 token 仍有效时采用 outcome.navigation；随后 wake + sync/refresh。
}
```

第二 public consumer 直接构造并挂载该 aside，并消费公开 request/outcome，不依赖测试宿主或私有类型。

## 本轮修复和验收

离屏检查发现把毫秒差值当秒的1000倍时长错误，已修正并用10,000→13,000毫秒必须为3秒的测试覆盖。原始字段保持不变，消息身份和源位置不参与单位换算。

完整原生回归还复现 SideChat 关闭时 `actionable run missing`：Composer 已捕获快照，辅助控件重新读取时 pointer 恰好撤销。现同一次刷新共用该快照，并关闭撤销/epoch 不匹配的动作。新增两个确定性用例分别覆盖撤销后旧快照只读，以及旧 frame 不能配新 epoch page receipt。也修正了 native 消息探针的挂载顺序，保持用户行→机器行→turn aside。

- 核心139项测试、UI206项测试（新增8个 footnote/dispatch、2个 frame 一致性、1个第二 consumer）、工具18项通过；root/UI 严格 Clippy、fmt、依赖边界和固定输入校验通过。
- 真实 Native/Pi/DSH 在拥有的临时会话与本地合成 provider 下，逐一完成共享目录→footnote 按钮→原 scope 一次派发→精确 boundary 分叉→新历史恢复；导航晚结果和两侧草稿隔离通过。Journal 使用 atSeq，Pi NativeConversation 使用 entryId。附带页前插、断线重连与消息保留的数据证明。
- 原生窗口通过 footnote 的合成指针点击、分叉导航、归档及草稿隔离；完整 fixture 的 Composer、SideChat、任务、子代理、Summary 回归5个标记通过。使用注入的文本服务事件，**物理 IME、VoiceOver、真实三端原生交互与混合阅读锚点均未由此证明**。详见同名 JSON 的源码/二进制/日志/图像散列。

## 保持的缺口

这是 turn 级 aside 与当前共享完成回合 Fork 的闭环。当前 turn footnote 输入未提供的评级、历史编辑/回退、目标/记忆细节不在 UI 伪造；复制正文等其余 footnote 入口、完整 Transcript/RunView 与 pane 消息组合仍要继续接入。测试宿主的单行探针不是正式消息文档。

混合文档阅读/前插锚点仍待 TekesUI #22；可选中富内容/原生映射仍待 #25。Markdown/附件展示范围、流到持久记录的可靠阅读关联和物理中文 IME 验收继续保留为独立工作，不能据本轮局部通过合并或打 UI release tag。
