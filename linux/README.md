# Lithe Linux client (GTK4/libadwaita)

Lithe 的 Linux 原生桌面客户端,基于 **Rust + GTK4 + libadwaita**(GNOME 官方
技术栈),与 macOS 侧的 Swift/AppKit 产品定位对等:全部产品行为(工作区枚举、
文件读写、Git 状态)通过共享核心 `rust/lithe-core` 的 JSON 命令完成,本目录
只拥有原生窗口与交互。

```text
GTK4/libadwaita widgets (linux/src/window.rs)
        |
        v
linux/src/core_bridge.rs  ->  lithe_core::execute_json  (与 macOS C ABI、
        |                                                 Windows platform_invoke
        `-- 同一命令面:workspace.snapshot / file.read /                   同构)
            file.write / git.status
```

当前为原生骨架 v1:打开工作区文件夹、文件列表、UTF-8 文件查看与 Ctrl+S 保存、
Git 分支显示。后续按此壳逐步补编辑器(GtkSourceView)、搜索、运行配置等能力。

## 构建

宿主机需要 GTK4/libadwaita 的 `-devel` 头文件。 Fedora:

```bash
sudo dnf install gtk4-devel libadwaita-devel
cd linux && cargo build
```

没有 root 时可用仓库自带的容器构建链(podman + Fedora 镜像,见
`scripts/build-linux-native.sh`),编译在容器内完成、产物落回本目录
`target/`,可在宿主机直接运行(运行时依赖宿主 GNOME 的 GTK4/libadwaita)。

## 运行

```bash
./target/debug/lithe-linux                          # 打开欢迎界面
./target/debug/lithe-linux /path/to/project         # 启动即打开工作区
```

应用支持 GApplication 的 `HANDLES_OPEN` 流程,也可通过桌面环境的"打开方式"
接收文件夹路径。
