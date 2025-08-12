{
  description = "Python program to show your google calendar next meeting";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  inputs.flake-utils.url = "github:numtide/flake-utils";

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system: let
      pkgs = nixpkgs.legacyPackages.${system};
      py = pkgs.python3;
      pyPkgs = pkgs.python3Packages;
    in {
      packages = {
        nextmeeting = pyPkgs.buildPythonApplication {
          pname = "nextmeeting";
          version = "2.0.0"; # keep in sync with pyproject.toml
          src = ../.;
          format = "pyproject";
          nativeBuildInputs = [ pyPkgs.hatchling ];
          propagatedBuildInputs = [
            pyPkgs.python-dateutil
            pyPkgs.gcalcli
          ];
          # sanity check import
          pythonImportsCheck = [ "nextmeeting" ];
          meta = {
            mainProgram = "nextmeeting";
            description = "Local server/client to display upcoming Google Calendar meetings in bars";
            homepage = "https://github.com/chmouel/nextmeeting";
            license = pkgs.lib.licenses.asl20;
          };
        };
        default = self.packages.${system}.nextmeeting;
      };
      devShells.default = pkgs.mkShell {
        packages = [
          pkgs.uv
          pyPkgs.python-dateutil
          pyPkgs.gcalcli
          pkgs.ruff
          pyPkgs.pytest
          pyPkgs.pytest-cov
        ];
        shellHook = ''
          export PYTHONPATH="$PWD:$PYTHONPATH"
          export PIP_PREFER_BINARY=1
        '';
      };
    });
}
