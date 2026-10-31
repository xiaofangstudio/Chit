# 参考底座说明

本文件夹存放 Chit 的开源底座，**只读参考，不提交、不打包、不复制进产品仓库**。

---

## 位置

| 项目 | 路径 | 许可证 |
| --- | --- | --- |
| Notepad4 | `reference\editor\notepad4` | BSD-3-Clause |

上游地址：`https://github.com/zufuliu/notepad4`

## 用途

Notepad4 是 C++ / Win32 写的原生文本编辑器，单 exe 约 300KB，启动在 30ms 量级，已内建多标签、编码识别、大文件模式、正则替换。

对 Chit 的价值：

- **改造量最小。** 需要的多数能力它已有，不必从零写。
- **技术栈与 Chit 的约束完全一致。** 原生 Win32、无运行时、单文件 exe，正好满足 [docs/04-资源占用预算.md](../docs/04-资源占用预算.md) 的全部预算。
- **编辑核心是 Scintilla**（HPND，BSD 系宽松），可并入。

**必须剔除的：**

- 任何联网检查更新
- 遥测与崩溃上报
- 与 Chit 无关的额外功能模块

## 许可证与署名

Notepad4 为 BSD-3-Clause，可改造并入，但必须：

1. 保留 Notepad4 自身的版权声明与许可证原文；
2. 保留其上游 Notepad2 与 Scintilla 的版权声明与许可证原文；
3. 不得用上游名义为 Chit 背书。

随包附 `THIRD_PARTY_NOTICES.md`，逐条列出。

**不许碰的：** Notepad++ 是 GPL，**只能读思路，一行代码都不许抄**。

## 下载

本目录当前为空，底座尚未克隆。重跑：

```powershell
.\clone-editor.ps1
.\clone-editor.ps1 -Proxy http://127.0.0.1:7890
```

脚本只做 `git clone --depth 1`，跳过已存在目录，不执行任何 `git add` / `commit` / `push`。

## 注意

这个文件夹是**只读参考**。不要修改它的内容，不要在里面做实验，不要把它提交到任何仓库。

---

*小方工作室 · Chit · 参考底座*