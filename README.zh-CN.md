# Starling（星语）· 安卓版《我的世界》中文汉化

> 本文档由 **DeepSeek** 生成。

**Starling（原名 Horizon Android）是把《我的世界：Java 版 26.3》轻量移植到安卓的开源项目**，由 [JuanLuci](https://github.com/JuanLuci) 与 [Vasysik](https://github.com/Vasysik) 共同创立。它是 [Horizon](https://github.com/horizon-rust/horizon)（用 Rust 从零重写的 Java 版）的 fork：读同样的存档、遵循同样的规则、生成同样的地形，并用差分测试与原版逐一对齐。你用自己的 Minecraft 文件来玩；它免费、开源、非官方，且在安卓上**原生运行**——没有 Java 虚拟机、没有 OpenGL 转译层，所以启动快、手机上玩起来流畅。

> ⚠️ 非官方 Minecraft 产品，与 Mojang / 微软无关。Starling 不含任何 Minecraft 代码、资源或数据：你需要自己合法拥有的《我的世界：Java 版 26.3》文件。

## 本仓库：starling-zh-cn（汉化版）

这是 [starling-client/starling](https://github.com/starling-client/starling) 的**简体中文汉化 fork**，在保留原版全部功能的基础上，把**启动器界面文案翻译成了简体中文**（约 830 处，覆盖主界面、登录、设置、安装流程、皮肤/衣柜、模组/商店、文件管理、触摸按键编辑器等）。

- **汉化方式**：直改源码文案，未改动游戏逻辑、品牌名与技术键。
- **汉化来源**：由 DeepSeek 生成，可能存在不当之处，欢迎提 issue 或 PR 修正。
- **分支**：`zh-cn`（汉化主分支）、`android`（触发构建用）。
- **构建**：复用上游 `android.yml`，在 GitHub Actions 中自动产出 APK。

## 下载

- **原版 Starling 正式版**：见 [starling-client/starling 的 Releases](https://github.com/starling-client/starling/releases)。
- **本汉化版 APK**：由本仓库的 [Actions](https://github.com/lihua4706-dot/starling-zh-cn/actions) 构建（Preview 版、debug 签名，可直接安装，但不能覆盖升级）。

**安装要求**：Android 8 或更新、兼容的图形驱动、约 2GB 剩余空间。首次启动会用手机从 Mojang 服务器下载并准备游戏文件（约 79 秒，视手机而定）。可用微软账号登录（需拥有 Java 版正版）或离线游玩。

## 已实现功能（摘要）

- **手机端准备**：微软账号登录，游戏文件从 Mojang 下载并在手机端准备（含全部语言）；准备期间可钓鱼或改设置。
- **游戏本体**：单人模式含原版世界生成、生物、村民、袭击、红石、全部指令、HUD、配方书，存档为原版格式。
- **触屏操控**：摇杆（浮动/固定）或方向键、点按交互、准星瞄准、F3 组合、拾取方块、玩家列表；布局可移动/缩放/淡出；支持陀螺仪瞄准。
- **自定义按键**：MCPE 与 GoyControl 两套方案、独立预设、撤销/重做、按钮尺寸/颜色/可见性、最多三个输入绑定。
- **基岩版背包**：创造模式六行物品 + 2×2 合成，一键拿整组。
- **皮肤与名字**：可改游戏内名字、从文件导入经典/细手臂皮肤。
- **手机键盘**：输入框自动唤起键盘，聊天/告示牌/书支持输入建议、自动更正、滑动输入与剪贴板。
- **文件管理**：世界、资源包、光影、模组、截图都在 `Starling` 文件夹；`.starling` / `.hmod` / `.hnmod` 文件一键打开导入。
- **服务器**：离线模式服、代理（如 Velocity）、插件服对话框、聊天按钮、自定义盔甲与家具、光影包（Complementary、MakeUp、BSL、Photon、Bliss）等。
- **模组**：原生模组 `.hnmod`（手机端编译）与内容模组 `.hmod`。
- **性能**：按机型自动配置，FPS/内存/温度悬浮窗。

详见上游 [README.md](README.md) 与[路线图](roadmap/starling.md)。

## 从源码构建

面向开发者。要求：Rust 1.90+（含 `aarch64-linux-android` target）、[cargo-ndk](https://github.com/bbqsrc/cargo-ndk)、Android SDK（build-tools 35.0.0、platform 36、NDK r28c）、JDK 25、Python 3.10+。

```sh
git clone -b starling https://github.com/starling-client/starling.git
# 构建脚本详见上游 packaging/android/build_apk.py 与 .github/workflows/android.yml
```

## 免责声明

本项目及其汉化均为非官方、非商业的开源行为，与 Mojang / 微软无关。请使用自己合法拥有的 Minecraft 文件。
