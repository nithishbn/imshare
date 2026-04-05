{
  description = "imshare - Generate signed, expiring share links for Immich";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };
        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "rust-analyzer" ];
        };
      in
      {
        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = "imshare";
          version = "0.1.0";
          src = ./.;

          cargoLock = {
            lockFile = ./Cargo.lock;
          };

          nativeBuildInputs = with pkgs; [
            pkg-config
          ];

          buildInputs = with pkgs; [
            openssl
          ];

          meta = with pkgs.lib; {
            description = "Generate signed, expiring share links for Immich via immich-public-proxy";
            license = licenses.mit;
          };
        };

        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            rustToolchain
            pkg-config
            openssl
            sqlite
          ];

          RUST_SRC_PATH = "${rustToolchain}/lib/rustlib/src/rust/library";
        };
      }
    ) // {
      nixosModules.default = { config, lib, pkgs, ... }:
        with lib;
        let
          cfg = config.services.imshare-verify;
        in
        {
          options.services.imshare-verify = {
            enable = mkEnableOption "imshare-verify proxy service";

            package = mkOption {
              type = types.package;
              default = self.packages.${pkgs.system}.default;
              description = "The imshare package to use";
            };

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
          };

          config = mkIf cfg.enable {
            users.users.${cfg.user} = {
              isSystemUser = true;
              group = cfg.group;
              home = "/var/lib/imshare";
              createHome = true;
            };

            users.groups.${cfg.group} = {};

            systemd.services.imshare-verify = {
              description = "imshare verification proxy";
              wantedBy = [ "multi-user.target" ];
              after = [ "network.target" ];

              serviceConfig = {
                Type = "simple";
                User = cfg.user;
                Group = cfg.group;
                ExecStart = "${cfg.package}/bin/imshare-verify";
                Restart = "always";
                RestartSec = "10s";
                EnvironmentFile = cfg.environmentFile;

                # Hardening
                NoNewPrivileges = true;
                PrivateTmp = true;
                ProtectSystem = "strict";
                ProtectHome = true;
                ReadWritePaths = [ "/var/lib/imshare" ];
              };
            };
          };
        };
    };
}
