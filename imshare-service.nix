# Add this to your NixOS configuration
# Either include this file or copy the contents into your configuration.nix

{ config, pkgs, ... }:

let
  # Build imshare from the local flake
  imshare = (builtins.getFlake "path:/root/imshare").packages.${pkgs.system}.default;

  # Generate a secret if needed (or use sops-nix/agenix for secrets management)
  # For now, we'll use an environment file
in
{
  # Create the imshare user
  users.users.imshare = {
    isSystemUser = true;
    group = "imshare";
    home = "/var/lib/imshare";
    createHome = true;
    description = "imshare verification service user";
  };

  users.groups.imshare = {};

  # Create the environment file directory
  systemd.tmpfiles.rules = [
    "d /etc/imshare 0755 root root -"
    "d /var/lib/imshare/.config/imshare 0755 imshare imshare -"
  ];

  # Environment file with secret
  # NOTE: Don't hardcode secrets here! Use sops-nix, agenix, or manually create /etc/imshare/env
  # Example: echo "IMSHARE_SECRET=$(openssl rand -base64 32)" | sudo tee /etc/imshare/env
  #
  # Uncomment and set your secret, or manage it externally:
  # environment.etc."imshare/env" = {
  #   text = ''
  #     IMSHARE_SECRET=your-secret-here
  #   '';
  #   mode = "0600";
  # };

  # Config file for the service
  environment.etc."imshare/config.toml" = {
    text = ''
      public_domain = "pub.nith.sh"
      default_ttl = "30d"
      db_path = "/var/lib/imshare/links.db"
      upstream = "http://localhost:3000"
      verify_port = 3001
    '';
    mode = "0644";
    user = "imshare";
    group = "imshare";
  };

  # Symlink config to user's config directory
  system.activationScripts.imshare-config = ''
    mkdir -p /var/lib/imshare/.config/imshare
    ln -sf /etc/imshare/config.toml /var/lib/imshare/.config/imshare/config.toml
    chown -R imshare:imshare /var/lib/imshare/.config
  '';

  # Systemd service
  systemd.services.imshare-verify = {
    description = "imshare JWT verification proxy";
    wantedBy = [ "multi-user.target" ];
    after = [ "network.target" ];
    wants = [ "network.target" ];

    serviceConfig = {
      Type = "simple";
      User = "imshare";
      Group = "imshare";

      # Use the binary from the flake build
      ExecStart = "${imshare}/bin/imshare-verify";

      # Restart policy
      Restart = "always";
      RestartSec = "10s";

      # Environment
      EnvironmentFile = "/etc/imshare/env";
      WorkingDirectory = "/var/lib/imshare";

      # Hardening
      NoNewPrivileges = true;
      PrivateTmp = true;
      ProtectSystem = "strict";
      ProtectHome = true;
      ReadWritePaths = [ "/var/lib/imshare" ];
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

  # Optional: Add the CLI to system packages so you can use it
  environment.systemPackages = [ imshare ];
}
