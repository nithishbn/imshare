# Example NixOS module for imshare-verify
#
# Usage:
#   1. Add this to your /etc/nixos/configuration.nix:
#      imports = [ /path/to/imshare/nixos-module.nix ];
#
#   2. Enable and configure:
#      services.imshare-verify.enable = true;
#
#   3. Create /etc/imshare/env with:
#      IMSHARE_SECRET=your-secret-here
#
#   4. Rebuild: sudo nixos-rebuild switch

{ config, lib, pkgs, ... }:

with lib;

let
  cfg = config.services.imshare-verify;

  # You'll need to build imshare first and reference the output
  # or import the flake. For manual installation:
  imshare-bin = "/usr/local/bin/imshare-verify";
in
{
  options.services.imshare-verify = {
    enable = mkEnableOption "imshare-verify proxy service";

    user = mkOption {
      type = types.str;
      default = "imshare";
      description = "User account under which imshare-verify runs";
    };

    group = mkOption {
      type = types.str;
      default = "imshare";
      description = "Group under which imshare-verify runs";
    };

    environmentFile = mkOption {
      type = types.path;
      default = "/etc/imshare/env";
      description = "Path to environment file containing IMSHARE_SECRET";
    };

    dataDir = mkOption {
      type = types.path;
      default = "/var/lib/imshare";
      description = "Directory for imshare data and database";
    };
  };

  config = mkIf cfg.enable {
    # Create user and group
    users.users.${cfg.user} = {
      isSystemUser = true;
      group = cfg.group;
      home = cfg.dataDir;
      createHome = true;
      description = "imshare verification service user";
    };

    users.groups.${cfg.group} = {};

    # Systemd service
    systemd.services.imshare-verify = {
      description = "imshare JWT verification proxy";
      wantedBy = [ "multi-user.target" ];
      after = [ "network.target" ];
      wants = [ "network.target" ];

      serviceConfig = {
        Type = "simple";
        User = cfg.user;
        Group = cfg.group;

        # Use the binary (adjust path as needed)
        ExecStart = imshare-bin;

        # Restart policy
        Restart = "always";
        RestartSec = "10s";

        # Environment
        EnvironmentFile = cfg.environmentFile;
        WorkingDirectory = cfg.dataDir;

        # Hardening options
        NoNewPrivileges = true;
        PrivateTmp = true;
        ProtectSystem = "strict";
        ProtectHome = true;
        ReadWritePaths = [ cfg.dataDir ];
        ProtectKernelTunables = true;
        ProtectKernelModules = true;
        ProtectControlGroups = true;
        RestrictAddressFamilies = [ "AF_INET" "AF_INET6" "AF_UNIX" ];
        RestrictNamespaces = true;
        LockPersonality = true;
        RestrictRealtime = true;
        RestrictSUIDSGID = true;
        RemoveIPC = true;
        PrivateMounts = true;
      };
    };

    # Ensure environment file directory exists
    system.activationScripts.imshare-env-dir = ''
      mkdir -p /etc/imshare
    '';
  };
}
