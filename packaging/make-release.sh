#!/usr/bin/env bash
set -euf
VERSION=${1-""}
PKGNAME=$(sed -n '/^name = / { s/name = "\(.*\)"/\1/ ;p;}' pyproject.toml)
PKGNAME=${PKGNAME//-/_} # replace dashes with underscores
echo "Package name is ${PKGNAME}"

bumpversion() {
  current=$(git describe --tags $(git rev-list --tags --max-count=1) || true)
  if [[ -z ${current} ]]; then
    current=0.0.0
  fi
  echo "Current version is ${current}"

  major=$(uv run --with semver python3 -c "import semver,sys;print(str(semver.VersionInfo.parse(sys.argv[1]).bump_major()))" ${current})
  minor=$(uv run --with semver python3 -c "import semver,sys;print(str(semver.VersionInfo.parse(sys.argv[1]).bump_minor()))" ${current})
  patch=$(uv run --with semver python3 -c "import semver,sys;print(str(semver.VersionInfo.parse(sys.argv[1]).bump_patch()))" ${current})

  echo "If we bump we get, Major: ${major} Minor: ${minor} Patch: ${patch}"
  read -p "To which version you would like to bump [M]ajor, Mi[n]or, [P]atch or Manua[l]: " ANSWER
  if [[ ${ANSWER,,} == "m" ]]; then
    mode="major"
  elif [[ ${ANSWER,,} == "n" ]]; then
    mode="minor"
  elif [[ ${ANSWER,,} == "p" ]]; then
    mode="patch"
  elif [[ ${ANSWER,,} == "l" ]]; then
    read -p "Enter version: " -e VERSION
    return
  else
    print "no or bad reply??"
    exit
  fi
  VERSION=$(uv run --with semver python3 -c "import semver,sys;print(str(semver.VersionInfo.parse(sys.argv[1]).bump_${mode}()))" ${current})
  [[ -z ${VERSION} ]] && {
    echo "could not bump version automatically"
    exit
  }
  echo "Releasing ${VERSION}"
}

[[ $(git rev-parse --abbrev-ref HEAD) != main ]] && {
  echo "you need to be on the main branch"
  exit 1
}
[[ -z ${VERSION} ]] && bumpversion

vfile=pyproject.toml
sed -i "s/^version = .*/version = \"${VERSION}\"/" ${vfile}
git commit -S -m "Release ${VERSION} 🥳" ${vfile} || true
git tag -s ${VERSION} -m "Releasing version ${VERSION}"
git push --tags origin ${VERSION}
git push origin main
rm -rf dist/
mkdir dist
uv build
gh release create ${VERSION} ./dist/${PKGNAME}-${VERSION}.tar.gz
uv publish -u __token__ -p $(pass show pypi/token)

./packaging/aur/build.sh
