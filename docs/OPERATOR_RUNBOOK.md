# ReadOS 运营手册（旗舰工作流）

本手册面向需要在 ReadOS WinUI 工作台中实际操作旗舰 MSP 工作流的运营人员，覆盖 T93 验收中“无法在 UI 层以下证明的 shell 交互”。服务层与重启/溯源的确定性证明见 `ReadOsFlagshipWorkflowIntegrationTests`；此处只描述可在界面中完成的交互。

## 旗舰场景

```text
导入 PDF
-> 选择大纲小节
-> workflow run extract-evidence
-> 审批产物写入
-> workflow run synthesize-evidence
-> 持久化 session/transcript/artifact/manifest
-> 重启并恢复工作区
-> 沿产物溯源回到原始文档页面
```

## 在 WinUI 工作台中操作

1. **导入资料**：点击顶部“导入资料”（或历史面板中的“导入资料”），选择 PDF。导入后文档出现在资料库中，并提供大纲。
2. **选择大纲小节**：在 Inspector（审查 Dock）的“证据/大纲”上下文里选中目标小节（例如 `3.2 Service Layer`）。选中后工作流目标变为可见。
3. **准备提取证据命令**：点击“提取证据”工作流动作，组合框草稿变为
   `workflow run extract-evidence --document current --outline "<id>" --artifact "/artifacts/workflows/<doc>-3-2-service-layer-evidence.json"`。
   该草稿同时出现在 Inspector 的“运行时”标签页与组合框中。
4. **运行并审批**：发送草稿执行 `RunMspCommandCommand`。写入 `/artifacts` 的命令默认进入待审批（`RequireConfirmation`），顶部“待审批 MSP 命令”按钮出现角标，审批浮层列出该命令。点击“通过”执行；点击“拒绝”记录一条 `Deny` 终态记录且不产生产物。
5. **合成证据**：在 Inspector 选中上一步生成的证据产物，点击“合成证据”工作流动作，草稿变为
   `workflow run synthesize-evidence --evidence "<evidence-path>" --artifact "/artifacts/workflows/evidence-synthesis.md"`。运行后调用模型并把结构化证据合成为 Markdown 产物。
6. **查看产物与溯源**：产物出现在 Inspector“产物”标签页；选中产物可展开溯源（证据 → 证据产物 → manifest → 原始文档页面），点击溯源项回到对应文档与页面。
7. **运行时抽屉**：底部“运行”抽屉显示精简 MSP transcript（命令文本 + 状态），点击可在 Inspector“运行时”标签页查看完整 stdout/stderr、诊断、恢复提示、效果、策略决策、耗时与产物。抽屉高度可拖拽并记忆，可固定常驻。

## 分支与异常（界面可见部分）

- **取消**：运行中的命令可在组合框主按钮切换为“停止”并通过 `CancelActiveMspCommandCommand` 取消，记录一条 `Canceled` 终态记录，不产生部分产物。
- **拒绝**：审批浮层“拒绝”记录 `Deny` 终态，不产生产物。
- **无效页**：大纲指向不存在的页范围时命令以 `reados.pdf.invalid_page` 稳定失败，不产生产物，并提示 `pdf inspect current`。
- **提供方失败**：合成阶段模型提供方抛错时，错误以 `reados.chat.model_provider_failed` 稳定失败，API 密钥与敏感请求不会出现在 stderr/诊断/审计/transcript/工作区中；重启后重试可恢复溯源。
- **重试**：失败后在工作区中重新提交同一合成命令，新记录追加（保留失败与成功两条终态），溯源与审计完整。

## 真实提供方 / 网络与打包进程重启

- 真实提供方与网络边界：在设置中填写提供方、Base URL、API Key（经 DPAPI 保护，不写入工作区 JSON/导出）与模型；关闭“离线响应”后合成证据会调用真实模型。
- 打包进程重启：旗舰场景在 CI 中通过 `.\scripts\verify-msp.ps1` 与 `.\scripts\package-windows.ps1` 以全新进程重启并恢复工作区来验证；交互式“重启应用”同样会重建服务与对象、重载持久化状态，溯源导航在重启后仍然可用。

## 验证

- 自动化：`tests/ReadOS.App.Tests/ViewModels/ShellViewModelTests.cs` 的 `Flagship_evidence_and_synthesis_workflows_are_visible_through_shell` 通过 `ShellViewModel` 驱动准备→运行→产物可见的完整链，作为最小 WinUI 冒烟。
- 服务层/重启/溯源：`ReadOsFlagshipWorkflowIntegrationTests` 覆盖取消、无效页、提供方失败、拒绝与可恢复重试，并验证重启后溯源。
- 完整验证：`dotnet test` 与 `.\scripts\verify-msp.ps1` 通过后，再运行 `.\scripts\package-windows.ps1` 完成打包进程冒烟。
