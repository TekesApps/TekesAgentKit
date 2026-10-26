# TekesUI 消费侧验收与正式 pin 比对

这些工具只在 AgentKit 消费侧工作，不修改 TekesUI、现有 manifest/.deps 或归档，不执行提升、Git 提交、推送、Issue 操作或发布。需要 Python 3.11+、Git、rustup；Rust 工具链从交付包的精确版本读取。当前 UI 仍是 `0.0.0/publish=false`。

## 一键复现交付包

在 AgentKit 根目录运行，显式给出已审阅归档和预期身份：

```sh
python3 scripts/verify-tekesui-handoff.py \
  --archive /absolute/agentkit-tekesui-0372186.tar.gz \
  --sha256 a220dfca80748f0b7d82f115697b59a156186852f5c433891e05cd7c65eb24d0 \
  --snapshot-sha256 03721866a362b1d31261ebbbc86140eb07e9e23320871c325234a5212591213d \
  --output /absolute/new-replay-directory \
  --target-dir /absolute/shared-cargo-target \
  --offscreen
```

`--output` 必须是工作区外的新目录；失败后保留源、日志和失败收据，重试用新目录。`--target-dir` 是多轮复用的包外构建缓存，不复制 target、不自动清理其他任务产物。允许 Cargo 正常下载 registry crates；这不是第三方依赖全离线供应包。

默认流程是：

1. 校验归档 SHA，拒绝路径穿越、链接、特殊文件和重复成员；在新目录解压，不执行包内 verify.py。
2. 核对 payload-manifest 的所有文件及显式公共 snapshot，拒绝额外文件、symlink、缺失和错字节。
3. 三套 Cargo metadata 确认所有本地依赖均在包内，再检查 headless/UI 所有权边界。
4. 运行 root/UI 格式检查；UI workspace、第二公开 API consumer、headless 的测试和严格 all-targets Clippy；构建 demo。
5. 可选 `--offscreen` 只生成 macOS Metal PNG，不打开原生窗口。结束再次核对全部源码身份。

输出 `receipt.json`、每步日志、命令/退出码/耗时/工具哈希、测试通过数与可选 PNG 哈希。命令失败立即停止，保留已完成步骤；命令超时按 `--timeout-seconds`（默认每步1200秒）终止本工具创建的进程组。源码和证据不自动删除。`native_interaction=false`、`promotion=false`，不把自动化或离屏结果当作物理 IME、VoiceOver、原生体验或性能结论。

该工具使用当前 AgentKit 的边界检查脚本检查解压对象，并记录工具 SHA。这样旧归档中依赖 `.git` 的历史检查脚本无需被篡改。`check-headless-boundaries.py` 本身也已支持无 Git 源码包：使用自身文件清单扫描，排除 `.git`、target、.deps、node_modules；若恰处于另一个 Git 仓库内部，不借用父仓库的 ignore/清单。依赖规则不放宽。

单独检查已解压对象：

```sh
python3 scripts/check-headless-boundaries.py --root /absolute/handoff/consumer
python3 scripts/check-ui-boundaries.py --root /absolute/handoff/consumer
```

前者可用 `--metadata /absolute/resolved.json` 消费该 root 的既有 Cargo metadata；workspace root 必须匹配。它不创建 `.git`。

## 比对公开不可变 pin

上游提供正式 commit 后，使用**先前已验收**的 manifest（037 的 manifest 位于该交付包中），不要重新 capture 活跃工作树：

```sh
python3 scripts/check-tekesui-pin.py \
  --manifest /absolute/reviewed/handoff/consumer/ui/tekesui-source.json \
  --repository https://github.com/TekesApps/TekesUI.git \
  --commit FULL_40_CHARACTER_COMMIT_SHA \
  --output /absolute/new-pin-comparison.json
```

此命令要求完整40位小写 commit SHA，拒绝分支名和可移动 tag 名。先通过不带凭证的 GitHub API 确认该 commit 可公开取得，再在临时 bare repo fetch 固定 commit，只读取 Git blob；不使用本地工作树/WIP 字节。允许其他显式 GitHub HTTPS 仓库用于消费比对，但报告保留完整 URL，项目是否接受该来源仍由审阅决定。

比对 manifest 中每个文件的 SHA，并列出：改变、缺失、不允许的 Git 文件类型（如 symlink）及被消费 crate 内新增文件。后者也需要审阅，不静默扩大输入。无关的其他 crate/文档不纳入这188文件验收范围。

| 结果 | 退出码 | 含义 |
|---|---|---|
| `matching-inputs` | 0 | 该公开 commit 满足本工具的已审阅输入比对；可进入显式 pin 迁移与按 pin 重验 |
| `review-required` | 1 | 有差异，逐项审阅后另行确认，不能沿用旧验收直接升级 |
| `failed` | 1 | 公开可获取性、网络、manifest 或 Git 读取失败；不能按匹配处理 |

输出文件必须不存在，网络/读取失败也保留收据。即使匹配，仍为 `promotion=false/release_authorized=false`：不改依赖、不创建 tag、不授权发布，也不自动关闭 Issue。正式 pin 迁移后应重新构建和验证；字节比对不能代替原生体验验收。

## 工具回归与本轮结果

```sh
python3 -m unittest discover -s scripts/tests -v
```

14项工具测试覆盖错误哈希/路径/链接/重复成员、manifest污染、依赖逃逸、嵌套父Git隔离、失败/超时收据、已提交blob与脏工作树隔离、缺失/改变/新增输入及Git symlink。测试只在临时目录创建 Git 夹具，不修改真实仓库；无网络依赖。已接入 CI，但本轮未推送，不能声称远端新门禁已运行。

另新增两项真实 SessionRuntime/RunController 故障回归，位于 `ui/demo/tests/failure_recovery.rs`：

- 首次分页超时不改变可见内容和草稿，不自动重试；显式重试沿用原 throughSequence/frontier 并成功恢复。
- 真实 assistant 流式期间通知目标关闭，通知转发停止；同一 Run 继续结算，原输入只出现一次。

它们不改变生产状态机，不代替原生 WindowWake 关闭体验。自动重连与有界恢复、过期快照、A/B晚到回填已有测试，本轮未重复扩写。

本轮分别保留：封存037交付包的自动化复现（UI28/公开consumer131/headless76）与 primary 旧冻结e4新增回归后的 UI29；二者不是同一源码集合，不混报计数。正式记录见 [等待期工具验证](tekesui-acceptance-tools-20261019.md)。

当前primary已经由授权迁移为公开4310937的git+rev依赖；guard支持并强制检查该模式下每个直接依赖的仓库/rev一致，禁止混用path。开发capture被禁用。封存037归档仍为其原始开发模式，验收工具兼容该历史输入，不改写它。新增两项pin模式工具回归后共16项，正式迁移证据见 [记录](tekesui-public-pin-verification-20261019.md)。
