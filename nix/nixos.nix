{
  lib,
  config,
  pkgs,
  ...
}:
let
  cfg = config.services.clicker-rs;
  inherit (lib) types;
in
{
  options.services.clicker-rs = {
    enable = lib.mkEnableOption "clicker-rs";
    package = lib.mkPackageOption pkgs "clicker-rs" { };
    group = lib.mkOption {
      type = types.str;
      default = "clicker";
      description = ''
        Group which users must be in to use {command}`clicker`.
      '';
    };
    settings = lib.mkOption {
      type = types.attrs;
      default = { };
      description = "Configuration for clicker-rs";
    };
  };

  config = lib.mkIf cfg.enable {
    environment = {
      systemPackages = [ cfg.package ];
      etc."clicker-rs/default.nix" = lib.mkIf (cfg.settings != { }) {
        text = lib.generators.toPretty { } cfg.settings;
      };
    };

    services.udev.extraRules = ''
      KERNEL=="uinput", OWNER="clicker-rs", MODE="0600"
    '';

    systemd.services.clicker-rs = {
      description = "Wayland autoclicker";
      after = [ "graphical.target" ];
      wants = [ "graphical.target" ];
      serviceConfig = {
        Group = cfg.group;
        RuntimeDirectory = "clicker-rs";
        RuntimeDirectoryMode = "0750";
        ExecStart = "${lib.getExe cfg.package}";
        Restart = "on-failure";

        # hardening

        # allow access to uinput
        DeviceAllow = [ "/dev/uinput" ];
        DevicePolicy = "closed";

        # allow creation of unix sockets
        RestrictAddressFamilies = [ "AF_UNIX" ];

        CapabilityBoundingSet = "";
        IPAddressDeny = "any";
        LockPersonality = true;
        MemoryDenyWriteExecute = true;
        NoNewPrivileges = true;
        PrivateNetwork = true;
        PrivateTmp = true;
        PrivateUsers = true;
        ProcSubset = "pid";
        ProtectClock = true;
        ProtectControlGroups = true;
        ProtectHome = true;
        ProtectHostname = true;
        ProtectKernelLogs = true;
        ProtectKernelModules = true;
        ProtectKernelTunables = true;
        ProtectProc = "invisible";
        ProtectSystem = "strict";
        RestrictNamespaces = true;
        RestrictRealtime = true;
        RestrictSUIDSGID = true;
        SystemCallArchitectures = "native";
        SystemCallFilter = [
          "@system-service"
          "~@privileged"
          "~@resources"
        ];
        UMask = "0077";
      };
    };

    users.groups."${cfg.group}" = { };
  };
}
