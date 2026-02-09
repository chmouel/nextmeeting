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
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        projectData = builtins.fromTOML (builtins.readFile ../pyproject.toml);
        python = pkgs.python3;
        pythonEnv = python.withPackages (
          ps: with ps; [
            python-dateutil
            caldav
          ]
        );
      in
      {
        packages = {
          nextmeeting = pkgs.python3Packages.buildPythonPackage {
            pname = "nextmeeting";
            version = projectData.project.version;
            src = pkgs.lib.cleanSource ../.;
            propagatedBuildInputs = [ pythonEnv ];
            pythonImportsCheck = [ "nextmeeting" ];
            format = "pyproject";
            nativeBuildInputs = [ pkgs.python3Packages.hatchling ];
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
            meta = {
              mainProgram = "nextmeeting";
            };
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
            pkgs.python3Packages.pytest
            pkgs.python3Packages.pylint
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
