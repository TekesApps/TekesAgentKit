# 正式 Transcript 的阅读事务候选联调（实验）

AgentKit 新增 `TranscriptContent::prepare_update` / `apply_prepared_update` 和公开借用计划 `TranscriptContentUpdate`。预检验证完整消息文档、scope、publication、epoch 及 namespace 容量，返回按展示顺序排列的稳定行 ID，不修改控件或预留 namespace。提交前再次检查基线与行身份，过期计划不改变树。消息、工具、footnote 和提示均使用既有规范化 presentation；没有新建历史镜像、滚动控制器或像素补偿。

此接口也可在默认公开固定 TekesUI 下使用。它不提供 endpoint 读取权限，也不代替库的 reading document attachment token。异步历史请求必须保留发起时的原 token；不能在返回后换成当前 token。计划只在同一 UI turn 内同步使用，不能在准备与提交之间修改组件状态。

## 独立候选

采用[接收记录](tekesui-reading-candidate-intake-20261020.md)中的449文件包，身份 `bd3ed1b0af81b044306af90d3594bb31110147b7194bda6053b1d2e185871c9a`。全部输入在执行前后逐项核验。候选源码只读；显式 path override 仅写入忽略目录下的 AgentKit 源码快照。Cargo metadata 确认八个 TekesUI 包均来自该输入，构建使用 `--locked --offline`，默认 manifest、lockfile 和 `tekesui-source.json` 不变。

候选新增 AX `required` 字段，隔离快照的原生宿主将公开 semantics.required 转发给平台节点；探针按已有 Preedit 契约提供空 clauses。这些是记录在收据中的候选 API 适配，不是对默认 pin 的静默兼容补丁。

独立重跑上游公开契约19项、composition consumer164项、严格 Clippy，以及一个 GPU 测试（内部48场景、192帧）。后者证明上游混合 Content 的几何/像素连续性，不证明 AgentKit 的像素、原生输入或 ClientKit 视觉一致。

## AgentKit 探针

[`scripts/probes/agentkit-reading-candidate.rs`](../scripts/probes/agentkit-reading-candidate.rs)直接挂载正式 `TranscriptContent` 与共享 `Composer`。测试宿主只组合公开 ContentHost/ScrollContent，不伪装成已完成的 RunView 接入。三个测试通过候选严格 Clippy：

- 追加与前插历史保持同一阅读行及其 y 位置（误差不超过1点）；旧 token 在调用回调前被拒绝，非法重复消息在修改阅读版本/控件树前被拒绝。
- A→B→A 在库事务回调内替换文档子树，恢复 A 的阅读项与位置；旧 A attachment 和 B token 均不能安装迟到分页。
- 从向上阅读状态通过 pointer 和公开 AXPress 激活“回到最新”，分别确认恰好一次 Activate、follow_latest 生效，Composer 焦点及合成 preedit 保留。

文字后端只使用 CoreText 测量，绘制回调为空。上述是正式组件的公开 API/几何测试；没有实际 AgentKit GPU 像素、真实 SEP 分页、原生窗口、实体键盘、候选窗、动量滚动或 VoiceOver 验收。AXPress 是公开事件调用，不是 VoiceOver 自动化。

## 重放

需要原449文件交付包和已缓存的 registry 依赖，参数必须明确指向独立候选源。工作目录必须不存在：

```sh
python3 scripts/test-reading-candidate.py \
  --source /absolute/verified-tekesui-source \
  --manifest /absolute/portable-manifest.json \
  --workdir /absolute/new-owned-agentkit-replay \
  --target-dir /absolute/owned-cargo-target
```

runner 保留 AgentKit 输入哈希、实际解析 metadata、测试/Clippy 日志及候选专用宿主适配。失败也保留收据；不升级 registry、不修改默认依赖、不据此提升候选。首次回放发现探针漏填已有 Preedit.clauses，且候选 AX 节点新增 required；第二次测试通过但严格 Clippy 拒绝了探针大写测试名；修正后重跑并加强 Activate/follow_latest 检查，replay-04 通过；补充 override 哈希收据后最终 replay-05 再次通过。

## 剩余交付

完整 RunView 更新还会触及 Composer 与其它可能拒绝的受控输入；TekesUI 阅读回调不回滚应用错误，不能直接把整个 fallible update 塞进回调。本次只在事务内提交已预检的 Transcript，Composer 位于阅读 owner 外。后续须完成正式 RunView 的更新协调、真实分页及原生阅读验收。

只读对照当前 ClientKit 的 TranscriptScrollCheckpoint / ThreadTranscriptPresenter+Scroll，仍须区分当前挂载恢复、跨挂载/重启的持久 checkpoint 与读取失败；本次仅证明当前进程的 A/B 重新挂载，不宣称持久恢复对齐。Tekes 宿主与 ClientKit 源码未修改。

默认依赖仍为公开固定 `8cf0840790334ba224afc0595897c74231c712e6`；候选未成为公开 Git pin。#22 与 #25 仍 OPEN，富内容选择和全文本映射另行处理。UI 保持 `0.0.0` / `publish=false`，未完成最终 review/CI 合并和阶段 tag。排版遵循机器左、用户及其外置 footnote 右；基础样式始终消费 TekesUI defaults/tokens/主题。

默认 pin 下完整 UI workspace 288 项、脚本26项、严格 UI Clippy/fmt、headless/UI 边界及实际解析的八包199输入验证通过。root 代码未改，本轮未重复 headless 全量166测试。汇总与日志哈希见[JSON 收据](rust-agent-ui-reading-candidate-20261020.json)。

脚本测试首次误用系统 Python 3.9，因缺少 tomllib 无法导入验收模块；保留失败日志，随后使用仓库验证流程所需的 bundled Python 原样重跑26项通过。

后续完整 RunView 更新边界见 [TekesUI #26](https://github.com/TekesApps/TekesUI/issues/26)：默认 pin 已有 Preedit.clauses，本文件先前将它描述为新增契约不准确，现已修正；新增字段差异是 AX required。另有公开 API 探针精确复现“编辑器 Composition 冲突不会拒绝 reading version”，这不是 RunView 成功验收。
