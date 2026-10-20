# 评价读取与最终消息身份

本轮补齐评价的共享读取基础，尚未开放评价按钮、切换/删除或备注编辑。发布基线仍是本轮核实的 AgentKit v0.3.8；以下为 PR 分支新增 API。TekesUI 保持固定公开 `8cf0840790334ba224afc0595897c74231c712e6`，没有修改 Kernel、TekesUI 或 Swift 源码。

## 契约与身份

对照 ClientKit `c289bc9d33edcd83a782ac17927a8c65a027eccd` 的 SessionFeedback、SessionEndpointRuntime 的 committed feedback/generation 检查和 DSHHostEndpoint.feedbackCall。新增 SessionMessageFeedbackRating、SessionMessageFeedback、SessionMessageFeedbackList 保留 messageId、opaque version、note 和可选 updatedAtMilliseconds；空备注与无备注不同。不把版本当作序号、时间或 UI revision。兼容 Pi 现有 up/down 评级别名，序列化为标准 positive/negative。

`SessionMessageFeedbackList::read` 要求 items，检查记录身份、非空 CAS version 和重复 messageId。缺字段、未知评级、非法备注或重复记录是错误；只有成功读取 items=[] 才表示已加载且无评价。没有从失败构造空列表，也不根据时间戳挑选重复记录。

`TranscriptSection::final_response()` 选择 completed 回合唯一可见、非 provisional 的最终 Assistant；final_output() 保持原有非空正文复制规则。`TurnFeedbackTarget::read` 再要求真实公共身份：

- Native/Pi：NativeEntry key、source position 的 entry_id 和 record_id 一致。
- Journal/DSH：record_id 等于 normalizer 从协议 message.id 明确保留的 metadata.feedbackMessageId。
- 缺少协议 ID 时，`assistant-{seq}` 仍可作展示用后备值，但不能作评价目标；相同字符串若确实来自协议 ID 则不会因拼写被拒绝。不按 ID 前缀猜测。

图片等非文本最终响应可以有评价目标，不能因此伪造可复制正文；这不代表图片预览或评价控件已经完成。target 是选择事实，不是 mutation authority。

## 公共读取

```rust,ignore
let source = original_binding.clone();
let frame = source.read_feedback().await?;
if frame.is_current(&source) {
    let committed = frame.records().get(target.message_id);
    // None 仅表示这个成功读取的列表没有该消息的评价。
}
```

FeedbackFrame 私有绑定原 RunBinding Weak 身份，公开 scope、publication、connection_epoch、records 及 put/delete 的 advertised facts。publication 在 IO 前分配，后发读取使旧读取失效；较早读取晚完成、重连/停止、不同 binding（即使同名 scope）及能力变化都不能得到 current frame。失败后可显式读取，不自动重试。UI 采用错误/结果时仍须保留原请求的宿主 mount/navigation 关联，不可将后台结果应用到新选中的 pane。

读取要求真实独立可寻址的 Run 和已公布的 feedback.list。有界子历史没有自己的 session/control authority，不以子 ID 调用反馈接口，也不回落到父 session。旧 frame 可以保留展示，但不能用 stale/未知记录准备 CAS。advertised put/delete 只表示读取时的能力，不授予执行许可。

第二公开 consumer 用 `committed_feedback(source, frame)` 区分过期/错源与成功空列表，不依赖 demo 私有类型。后续 dispatcher 仍需在 IO 前核对 pointer、target、epoch、最新能力及已提交的 CAS version；toggle/delete 与备注策略尚未实现。

## DSH 包裹修复

首轮真实读取在 DSH 报 missing field items，原因为 Rust 适配器直接返回 `{ok,value/error}` 包裹。对照 ClientKit 后已在 endpoint-dsh 修复公共 feedback.list/put/delete：成功 list/put 返回校验后的 value，delete 返回 null；内层失败成为 Error::Remote，并保留 code/current（含 version-conflict 的当前记录）。无效包裹或记录报错，不能变成空列表/成功删除。其它扩展结果不变。

这是公共 feedback.* 结果的兼容性修正：曾自行按 DSH value 解包的调用方应移除该层，并处理错误返回。没有把厂商格式识别放进 UI，也没有修改 DSH 本身。put/delete 的包裹处理本轮由适配器单测验证，新增真实读取探针没有写入评价。

## 验证与剩余工作

[JSON 收据](rust-agent-ui-feedback-read-20261020.json)保存结果。150 项 headless、250 项 UI（含第二 consumer）、21 项脚本、两套严格 Clippy/fmt、固定输入与依赖边界检查通过。新回归覆盖解析、占位/原生 ID、并发读、重连、同名 binding 隔离、失败/空成功、缺能力不 IO，以及有界历史不冒充会话。

在既有三端命令中加 `--ui-feedback-read`（需要 --ui-demo、--ui-components、--ui-transcript-content）可跑只读探针。Native、Pi、DSH 实际 transport 均已成功读取 typed list；本次隔离会话记录为空，最终目标数分别为1、2、2。非空记录/备注由独立协议 fixture 覆盖，不能把真实空列表扩大为真实备注恢复验收。既有完整生命周期仍运行原有创建/发送/Pi CAS 等测试；新增读取探针不做评价 mutation。原生六项组件回归和系统拼音软件探针也通过，未宣称评价控件的原生交互验收。

继续待做：评价 CAS put/delete、同评价 toggle、备注编辑/失败展示及控件；TekesUI #22 混合阅读锚点、#25 可选中富内容；完整 Run/Transcript、review/CI/合并与阶段 tag。UI 保持 experimental。
