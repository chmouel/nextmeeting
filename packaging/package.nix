{
  lib,
  buildPythonPackage,
  hatchling,
  python-dateutil,
  caldav,
  ruff,
  mypy,
}:
let
  projectData = builtins.fromTOML (builtins.readFile ../pyproject.toml);
in
buildPythonPackage {
  pname = projectData.project.name;
  version = projectData.project.version;
  src = lib.cleanSource ../.;
  pyproject = true;

  build-system = [ hatchling ];

  dependencies = [
    python-dateutil
    caldav
  ];

  # prefer prebuilt wheels for the dev tooling
  preBuildPhases = [ "preferWheelPhase" ];
  preferWheelPhase = ''
    export PIP_PREFER_BINARY=1
  '';

  nativeCheckInputs = [
    ruff
    mypy
  ];

  pythonImportsCheck = [ "nextmeeting" ];

  meta = {
    description = projectData.project.description;
    homepage = "https://github.com/chmouel/nextmeeting";
    license = lib.licenses.asl20;
    mainProgram = "nextmeeting";
  };
}
