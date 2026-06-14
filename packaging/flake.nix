{
  description = "Python program to show your google calendar next meeting";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  inputs.flake-utils.url = "github:numtide/flake-utils";

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
    }:
    {
      overlays.default = final: prev: {
        nextmeeting = final.python3Packages.callPackage ./package.nix { };
      };
    }
    // flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        pythonEnv = pkgs.python3.withPackages (
          ps: with ps; [
            python-dateutil
            caldav
          ]
        );
      in
      {
        packages = {
          nextmeeting = pkgs.python3Packages.callPackage ./package.nix { };
          default = self.packages.${system}.nextmeeting;
        };

        devShells.default = pkgs.mkShell {
          packages = [
            pkgs.uv
            pythonEnv
            # Development tools
            pkgs.ruff
            pkgs.python3Packages.mypy
          ];
          shellHook = ''
            export PYTHONPATH="$PWD:$PYTHONPATH"
            # Configure wheel preferences
            export PIP_PREFER_BINARY=1
          '';
        };
      }
    );
}
