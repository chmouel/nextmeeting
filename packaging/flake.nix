{
  description = "Python program to show your google calendar next meeting";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  inputs.flake-utils.url = "github:numtide/flake-utils";
  inputs.poetry2nix = {
    url = "github:nix-community/poetry2nix";
    inputs.nixpkgs.follows = "nixpkgs";
    inputs.flake-utils.follows = "flake-utils";
  };

  outputs = {
    self,
    nixpkgs,
    flake-utils,
    poetry2nix,
  }:
    flake-utils.lib.eachDefaultSystem (system: let
      p2n = import poetry2nix {inherit pkgs;};
      python = pkgs.python3;
      projectDir = ../.;
      overrides = p2n.overrides.withDefaults (final: prev: {
        # use wheels to build ruff & mypy
        ruff = prev.ruff.override {
          preferWheel = true;
        };
        mypy = prev.mypy.override {
          preferWheel = true;
        };
      });
      poetry_env = p2n.mkPoetryEnv {
        inherit python projectDir overrides;
      };
      poetry_app = p2n.mkPoetryApplication {
        inherit python projectDir overrides;
      };
      pkgs = nixpkgs.legacyPackages.${system};
    in {
      packages = {
        nextmeeting = poetry_app;
        default = self.packages.${system}.nextmeeting;
      };
      devShells.default =
        pkgs.mkShell {packages = [pkgs.poetry poetry_env];};
    });
}
