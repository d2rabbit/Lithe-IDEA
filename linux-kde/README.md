# Lithe KDE client (Qt 6 / C++)

Lithe 的 KDE 侧原生桌面客户端,基于 **C++20 + Qt 6 Widgets**,通过
**lithe-core 的稳定 C ABI 静态库**(`lithe_core_execute_json` /
`lithe_core_free_string`,见 `rust/lithe-core/src/runtime/ffi.rs`)消费与
GNOME 客户端、Windows、macOS 完全相同的共享命令面
(`workspace.snapshot` / `file.read` / `file.write` / `git.status`)。

当前为骨架 v1:打开工作区文件夹、文件列表、UTF-8 文件查看与 Ctrl+S 保存、
Git 分支显示。后续按此壳补 QML/Kirigami 界面、语法高亮(KSyntaxHighlighting)、
多标签等能力。

## 构建

需要 cmake、g++、Qt 6(Qt6::Widgets / Qt6::Concurrent)开发包,以及一份
预构建的 lithe-core 静态库(容器内或宿主):

```bash
# 1) 构建共享核心静态库(Fedora 容器或任意装有 rust 的环境)
cargo build --manifest-path ../rust/Cargo.toml -p lithe-core

# 2) 构建客户端(静态库路径可用 -DLITHE_CORE_LIB=... 覆盖)
cmake -B build -DCMAKE_BUILD_TYPE=Debug
cmake --build build
```

宿主机没有 Qt6 devel 包时,可复用 `scripts/build-linux-native.sh` 的容器模式
(把 CONTAINER 的 dnf 列表换成 `gcc-c++ cmake qt6-qtbase-devel qt6-qtwayland`),
或在容器内运行:`podman exec -e WAYLAND_DISPLAY=... lithe-linux-build
./build/lithe-kde`。

## 运行

```bash
./build/lithe-kde                       # 欢迎界面
./build/lithe-kde /path/to/project      # 启动即打开工作区
```
