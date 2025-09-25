daemon:
    cargo build --bin daemon
    sudo --preserve-env=WAYLAND_DISPLAY,XDG_RUNTIME_DIR ./target/debug/daemon
daemon-verbose:   
    cargo build --bin daemon
    sudo --preserve-env=WAYLAND_DISPLAY,XDG_RUNTIME_DIR ./target/debug/daemon --log-level debug
