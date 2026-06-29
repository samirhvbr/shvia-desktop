{
  description = "SHVIA-DESKTOP - Claude Desktop for Linux";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixpkgs-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
  };

  outputs = inputs:
    inputs.flake-parts.lib.mkFlake { inherit inputs; } {
      systems = [ "x86_64-linux" "aarch64-linux" ];

      perSystem = { pkgs, system, ... }: let
        node-pty = pkgs.callPackage ./nix/node-pty.nix { };
        shvia-desktop = pkgs.callPackage ./nix/shvia-desktop.nix {
          inherit node-pty;
        };
        shvia-desktop-fhs = pkgs.callPackage ./nix/fhs.nix {
          inherit shvia-desktop;
        };
      in {
        _module.args.pkgs = import inputs.nixpkgs {
          inherit system;
          config.allowUnfreePredicate = pkg: builtins.elem (inputs.nixpkgs.lib.getName pkg) [
            "shvia-desktop"
          ];
        };

        packages = {
          inherit shvia-desktop shvia-desktop-fhs;
          default = shvia-desktop-fhs;
        };
      };

      flake = {
        overlays.default = final: prev: let
          node-pty = final.callPackage ./nix/node-pty.nix { };
        in {
          shvia-desktop = final.callPackage ./nix/shvia-desktop.nix {
            inherit node-pty;
          };
          shvia-desktop-fhs = final.callPackage ./nix/fhs.nix {
            shvia-desktop = final.shvia-desktop;
          };
        };
      };
    };
}
