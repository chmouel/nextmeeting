{
  description = "Python program to show your google calendar next meeting";
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";

    pyproject-nix = {
      url = "github:nix-community/pyproject.nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    pyproject-build-systems = {
      url = "github:pyproject-nix/build-system-pkgs";
      inputs.pyproject-nix.follows = "pyproject-nix";
      inputs.uv2nix.follows = "uv2nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    uv2nix = {
      url = "github:adisbladis/uv2nix";
      inputs.pyproject-nix.follows = "pyproject-nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };
  outputs = {
    nixpkgs,
    flake-utils,
    uv2nix,
    pyproject-nix,
    pyproject-build-systems,
    ...
  }:
    flake-utils.lib.eachDefaultSystem (system: let
      inherit (nixpkgs) lib;
      pkgs = nixpkgs.legacyPackages.${system};
      python = pkgs.python312;
      # setup uv2nix workspace & overlay
      workspace = uv2nix.lib.workspace.loadWorkspace {workspaceRoot = ../.;};
      overlay = workspace.mkPyprojectOverlay {
        # Prefer prebuilt binary wheels as a package source.
        sourcePreference = "wheel";
      };
      pythonSet =
        # Use base package set from pyproject.nix builders
        (pkgs.callPackage pyproject-nix.build.packages {
          inherit python;
        })
        .overrideScope
        (
          lib.composeManyExtensions [
            pyproject-build-systems.overlays.default
            overlay
          ]
        );
    in {
      packages = {
        ${system}.default = pythonSet.mkVirtualEnv "nextmeeting" workspace.deps.default;
      };
    });
}
