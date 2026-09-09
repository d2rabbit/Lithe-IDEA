#!/usr/bin/env bash
# Builds the native Linux client (linux/) inside a Fedora podman container.
#
# Useful when the host lacks gtk4-devel/libadwaita-devel: the toolchain lives
# in the container, while the produced binary runs on the host desktop (it
# links against the host's GNOME GTK4/libadwaita runtime via the standard
# dynamic linker).
#
# Usage: scripts/build-linux-native.sh [build|run] [extra cargo args...]
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
IMAGE="docker.io/library/fedora:latest"
CONTAINER="lithe-linux-build"

command -v podman >/dev/null 2>&1 || { echo "podman is required" >&2; exit 1; }

action="${1:-build}"
shift || true

if [ "$(podman inspect -f '{{.State.Running}}' "$CONTAINER" 2>/dev/null)" != "true" ]; then
    podman rm -f "$CONTAINER" >/dev/null 2>&1 || true
    # keep-id keeps compiled artifacts owned by the host user; the label
    # disable works around SELinux relabel failures on large source trees.
    podman run -d --name "$CONTAINER" --userns=keep-id \
        --security-opt label=disable \
        -v "$ROOT_DIR":/home/dev/code \
        "$IMAGE" sleep infinity >/dev/null
    podman exec -u 0 "$CONTAINER" \
        dnf install -y --setopt=install_weak_deps=False \
        rust cargo gtk4-devel libadwaita-devel git
fi

if [ "$action" = "run" ]; then
    # Run the client on the host desktop through the container's copy.
    exec podman exec \
        -e WAYLAND_DISPLAY="${WAYLAND_DISPLAY:-wayland-0}" \
        -e XDG_RUNTIME_DIR=/run/user/"$(id -u)" \
        -v /run/user/"$(id -u)":/run/user/"$(id -u)":Z \
        -w /home/dev/code/Lithe-IDEA/linux \
        "$CONTAINER" ./target/debug/lithe-linux "$@"
fi

podman exec \
    -e CARGO_HOME=/home/dev/code/Lithe-IDEA/linux/.cargo-home \
    -w /home/dev/code/Lithe-IDEA/linux \
    "$CONTAINER" cargo build "$@"
