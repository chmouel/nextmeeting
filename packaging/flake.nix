{
  description = "Python program to show your google calendar next meeting";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  inputs.flake-utils.url = "github:numtide/flake-utils";

  outputs = {
    self,
    nixpkgs,
    flake-utils,
  }:
    flake-utils.lib.eachDefaultSystem (system: let
      pkgs = nixpkgs.legacyPackages.${system};
      python = pkgs.python3;
      pythonEnv = python.withPackages (ps: with ps; [
        python-dateutil
      ]);
    in {
      packages = {
        nextmeeting = pkgs.python3Packages.buildPythonPackage {
          pname = "nextmeeting";
          version = "1.5.5";
          src = ../.;
          propagatedBuildInputs = [ pythonEnv ];
          pythonImportsCheck = [ "nextmeeting" ];
          format = "pyproject";
          nativeBuildInputs = [
            pkgs.python3Packages.hatchling
          ];
          # Add development tools to the package
          checkInputs = [
            pkgs.ruff
            pkgs.python3Packages.mypy
          ];
          # Configure wheel preferences for development tools
          preBuildPhases = [ "preferWheelPhase" ];
          preferWheelPhase = ''
            export PIP_PREFER_BINARY=1
          '';
        };
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
    });
}
