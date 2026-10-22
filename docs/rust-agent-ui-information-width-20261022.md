# 只读输入信息的剩余行宽（2026-10-22，实验状态）

更正上一阶段将只读信息也限制为 40% 的规则。当前 `InputMessage` 原时间/送达信息使用
原复制按钮和间距之前的剩余行宽；没有复制入口时可使用整列。SDK 继续负责单行省略，
原 source、语义名称、稳定信息/复制控件、scope/key/text 与一次性复制 admission 不变。
不新增滚动、焦点、字体、IME 或业务状态机，不修改 TekesUI。

## 依据与公开契约

ClientKit `TranscriptFootnoteTarget.footnoteInformation` 返回普通 `labelRow`，没有 actionItem。
MetalTextKit `MetalVirtualLayout.appendAside` 的 input/message lane 从右向左排列：
`actionItem.width == .label`（例如反馈备注）上限为列宽 40%；普通只读信息走 nil 分支，
使用当前 cursor 到列起点的剩余宽度。不能把两种规则混用。
只读来源及 Tekes host adapter 的哈希保存在本阶段 `source-refs.json` / 程序收据。

不改变公开 API。仍需 `TextSurfaceHost::shape_label`；缺少服务由 SDK 明确返回
`MissingLabelServices`。`aside_id()` / `information_text()` 和旧信息 leaf ID 保留。
输入脚注原阶段 JSON、程序和成功/失败记录保持原样，不能视为本次程序的验收证据。

## 验证与边界

完整 UI workspace 520 passed、1 既有 #49 ignored；strict all-target Clippy 和 demo build 通过。
第二公开 consumer 覆盖 4 主题、3 字号、180/320/770 列宽、正文和纯附件：
完整长信息单行省略但不受 40% 错误上限，保留复制所需空间；原 copy 与信息同一右对齐带。
更新、复制及 label 变化不替换另一原 Composer 的实例/焦点/完整 marked baseline；
缺少 label 服务明确失败。长备注的独立 40% 回归仍通过。

最终冻结程序 `AgentKitInformationWidth20261022.app`，SHA256
`08d101e1be53ab0d47d9d87cfc20d6a7657f9adb180e18e011cfd71a67b4904b`。
三端离屏结果见[机器记录](rust-agent-ui-information-width-20261022.json)。

本程序没有新的原生窗口/系统拼音通过声明；上一个程序 phase 0 一直 inactive 的失败保留。
分页、reveal、焦点、组字的当前程序原生验收仍待前台可用时补齐。
默认文字按钮的窄列全部可见性、#47/#49、ProcessContainer #48、其余完整 Views 仍未完成。
精确公开 TekesUI 继续固定 3bef667；已发布 AgentKit 契约仍为 v0.3.8，UI 实验版本 0.0.0。
PR #10 保持 draft，无合并或 tag。

复用现有 UI/root target。日志、失败、固定源和程序保留；未新建 target。
