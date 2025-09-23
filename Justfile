daemon:
    cargo build --bin daemon
    sudo --preserve-env=WAYLAND_DISPLAY,XDG_RUNTIME_DIR ./target/debug/daemon
