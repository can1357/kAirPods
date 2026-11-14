{
  description = "kAirPods: kairpodsd user service + Plasma 6 plasmoid (org.kairpods.plasma)";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    crane.url = "github:ipetkov/crane";
    home-manager.url = "github:nix-community/home-manager";
    home-manager.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs = { self, nixpkgs, flake-utils, crane, home-manager, ... }:
    flake-utils.lib.eachDefaultSystem
      (system:
        let
          pkgs = import nixpkgs {
            inherit system;
          };
          craneLib = crane.mkLib pkgs;

          # check that the service unit file has not changed
          serviceUnitHashValid =
            let
              serviceUnitPath = ./service/systemd/user/kairpodsd.service;
              expectedServiceUnitHash = "sha256-aWc/ii+K8S6wElIq1tm0Rr3aUzXWGsYUSFy9idGTXQY=";
              actualServiceUnitHash = builtins.convertHash rec {
                hashAlgo = "sha256";
                toHashFormat = "sri";
                hash = builtins.hashFile hashAlgo serviceUnitPath;
              };
            in
            if actualServiceUnitHash != expectedServiceUnitHash then
              throw ''
                kAirPods flake error: service/systemd/user/kairpodsd.service changed.

                Expected hash: "${expectedServiceUnitHash}"
                Actual hash: "${actualServiceUnitHash}"

                Update the Home Manager `systemd.user.services.kairpodsd` definition to match,
                then update `expectedServiceUnitHash`.
              ''
            else
              true;

          # if the service unit file is valid, build the service package
          kairpodsd =
            if serviceUnitHashValid then
              craneLib.buildPackage
                {
                  src = craneLib.cleanCargoSource ./service;
                  nativeBuildInputs = with pkgs; [
                    pkg-config
                  ];
                  buildInputs = with pkgs; [
                    dbus
                    bluez
                  ];
                }
            else null;

          # Plasma searches $XDG_DATA_DIRS/share/plasma/plasmoids/<id> for plasmoids
          kairpods-plasmoid = pkgs.stdenvNoCC.mkDerivation rec {
            pname = "org.kairpods.plasma";
            version = "0.1.0";
            src = ./plasmoid;

            nativeBuildInputs = [ pkgs.gettext ];

            buildPhase = ''
              runHook preBuild
              bash translate/build.sh
              runHook postBuild
            '';

            installPhase = ''
              runHook preInstall
              mkdir -p "$out/share/plasma/plasmoids/${pname}"
              cp -r metadata.json contents "$out/share/plasma/plasmoids/${pname}/"
              runHook postInstall
            '';

            meta = with pkgs.lib; {
              description = "kAirPods Plasma 6 widget (plasmoid) for AirPods battery display";
              platforms = platforms.linux;
            };
          };

        in
        {
          packages = {
            kairpodsd = kairpodsd;
            plasmoid = kairpods-plasmoid;
          };
        }
      )
    //
    {
      # ---------------------------
      # Home Manager module
      # ---------------------------
      homeModules.default = { config, lib, pkgs, ... }:
        let
          cfg = config.services.kairpods;
        in
        {
          options.services.kairpods = {
            enable = lib.mkEnableOption "kAirPods (kairpodsd user service + Plasma plasmoid)";
            package = lib.mkOption {
              type = lib.types.package;
              default = self.packages.${pkgs.stdenv.hostPlatform.system}.kairpodsd;
              defaultText = "kAirPods kairpodsd package from this flake";
              description = "Package providing the kairpodsd binary.";
            };
            plasmoidPackage = lib.mkOption {
              type = lib.types.package;
              default = self.packages.${pkgs.stdenv.hostPlatform.system}.plasmoid;
              defaultText = "kAirPods plasmoid package from this flake";
              description = "Package providing the org.kairpods.plasma plasmoid.";
            };
            autoStart = lib.mkOption {
              type = lib.types.bool;
              default = true;
              description = "Whether to enable and start the systemd --user service.";
            };
          };

          config = lib.mkIf cfg.enable {
            home.packages = [
              cfg.package
              cfg.plasmoidPackage
            ];

            systemd.user.services.kairpodsd = lib.mkIf cfg.autoStart {
              Unit = {
                Description = "kAirPods D-Bus Service";
                After = [ "graphical-session.target" ];
              };
              Service = {
                Type = "dbus";
                BusName = "org.kairpods";
                ExecStart = "${cfg.package}/bin/kairpodsd";
                Restart = "on-failure";
                RestartSec = "5";
                PrivateTmp = "yes";
                NoNewPrivileges = "yes";
              };
              Install = {
                WantedBy = [ "default.target" ];
              };
            };
          };
        };
    };
}
