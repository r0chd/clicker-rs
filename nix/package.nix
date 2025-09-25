{
  rustPlatform,
  lib,
  pkg-config,
  wayland,
}:
let
  cargoToml = builtins.fromTOML (builtins.readFile ../Cargo.toml);
in
rustPlatform.buildRustPackage {
  pname = "clicker-rs";
  inherit (cargoToml.workspace.package) version;

  cargoLock = {
    lockFile = ../Cargo.lock;
    outputHashes = {
      "tvix-eval-0.1.0" = "sha256-2uNjqycyGa07RYDYfo7i6rk6zgC1pCfaAgoMTEoF6q0=";
    };
  };

  src = lib.cleanSourceWith {
    src = ../.;
    filter =
      path: type:
      let
        relPath = lib.removePrefix (toString ../. + "/") (toString path);
      in
      lib.any (p: lib.hasPrefix p relPath) [
        "daemon"
        "ctl"
        "common"
        "Cargo.toml"
        "Cargo.lock"
      ];
  };

  nativeBuildInputs = [
    pkg-config
    rustPlatform.bindgenHook
  ];

  buildInputs = [
    wayland
  ];

  buildPhase = ''
    cargo build --release --workspace
  '';

  installPhase = ''
    install -Dm755 target/release/daemon $out/bin/clickerd
    install -Dm755 target/release/ctl $out/bin/clicker
  '';

  meta = {
    description = "Mox desktop environment notification system";
    homepage = "https://github.com/r0chd/clicker-rs";
    license = lib.licenses.mit;
    maintainers = builtins.attrValues { inherit (lib.maintainers) r0chd; };
    platforms = lib.platforms.linux;
    mainProgram = "clickerd";
  };
}
