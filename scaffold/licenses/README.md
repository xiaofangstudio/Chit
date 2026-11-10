# licenses —— 许可证与署名

## 自研代码

- 本仓库全部源码以 **MIT** 许可证发布：见 [LICENSE-MIT.txt](LICENSE-MIT.txt)。
- 出品：小方工作室。非营利。

## 第三方组件

当前版本（M0 自研骨架）**没有并入任何第三方源码**：

| 组件 | 用途 | 许可证 | 是否随产品分发 |
| --- | --- | --- | --- |
| windows-rs | 仅开发期 API 绑定（Cargo 依赖，不修改、不并入源码） | MIT / Apache-2.0 双许可 | 否（编译期使用，产物为自研代码） |

按 docs/03 首选路线并入 Notepad4 / Scintilla 底座后，需在此逐条补充：

- Notepad4 —— BSD-3-Clause（保留版权声明与许可证原文）
- Notepad2 —— BSD-3-Clause
- Scintilla —— HPND

> 红线（AGENTS.md 第九条）：GPL / AGPL 一律不进仓库。Notepad++ 只读思路，一行代码不抄。
