set shell := ["bash", "-euo", "pipefail", "-c"]

# List available recipes
default:
    @just --list

# Print the current crate version from Cargo.toml
version:
    @python3 -c "import tomllib; print(tomllib.load(open('Cargo.toml', 'rb'))['package']['version'])"

# Run test suites for both opencv and no-default-features builds
check:
    cargo test --features opencv
    cargo test --no-default-features

# Build release binaries (with and without opencv) into dist/
build-bins:
    #!/usr/bin/env bash
    set -euo pipefail
    VERSION="$(just version)"
    echo "Building warpd-rs v${VERSION}..."
    mkdir -p dist

    # With opencv (default features)
    cargo build --release
    cp target/release/warpd-rs "dist/warpd-rs-v${VERSION}"

    # Without opencv
    cargo build --release --no-default-features
    cp target/release/warpd-rs "dist/warpd-rs-nocv-v${VERSION}"

    echo "Built binaries:"
    ls -lh "dist/warpd-rs-v${VERSION}" "dist/warpd-rs-nocv-v${VERSION}"

# Bump version in Cargo.toml & Cargo.lock and create a signed release commit
[arg('patch', long='patch', value='true', help='Bump patch version (x.y.Z)')]
[arg('minor', long='minor', value='true', help='Bump minor version (x.Y.0)')]
[arg('major', long='major', value='true', help='Bump major version (X.0.0)')]
[arg('set_version', long='version', help='Set explicit SemVer version (e.g. 0.3.0)')]
[arg('no_commit', long='no-commit', value='true', help='Update Cargo.toml and Cargo.lock without committing')]
[arg('dry_run', long='dry-run', value='true', help='Show planned version bump without modifying files or git')]
bump patch='false' minor='false' major='false' set_version='' no_commit='false' dry_run='false':
    #!/usr/bin/env bash
    set -euo pipefail

    CURRENT="$(just version)"
    NEXT="$(just _resolve-version "{{patch}}" "{{minor}}" "{{major}}" "{{set_version}}")"
    TAG="v${NEXT}"
    MSG="chore💤: release warpd-rs ${TAG}"

    if git rev-parse -q --verify "refs/tags/${TAG}" >/dev/null; then
        echo "error: tag ${TAG} already exists locally" >&2
        exit 1
    fi

    if [[ "{{dry_run}}" == "true" ]]; then
        if [[ -n "$(git status --porcelain)" ]]; then
            echo "[dry-run] warning: git working tree is dirty (would fail without --dry-run)"
        fi
        echo "[dry-run] bump: ${CURRENT} -> ${NEXT}"
        echo "[dry-run] would update Cargo.toml and refresh Cargo.lock"
        if [[ "{{no_commit}}" != "true" ]]; then
            echo "[dry-run] would commit: git commit -S -m \"${MSG}\""
        fi
        exit 0
    fi

    if [[ "{{no_commit}}" != "true" ]]; then
        just _ensure-clean
    fi

    python3 - "${CURRENT}" "${NEXT}" <<'PY'
    import re
    import sys
    from pathlib import Path

    current, target = sys.argv[1], sys.argv[2]
    path = Path("Cargo.toml")
    text = path.read_text(encoding="utf-8")

    # Replace only the version field inside [package]
    pkg_match = re.search(r'(?ms)^(\[package\]\s*\n.*?)^(version\s*=\s*")' + re.escape(current) + r'(")', text)
    if not pkg_match:
        sys.exit(f"error: could not locate [package] version = \"{current}\" in Cargo.toml")

    updated = (
        text[:pkg_match.start(2)]
        + pkg_match.group(2)
        + target
        + pkg_match.group(3)
        + text[pkg_match.end(3):]
    )
    path.write_text(updated, encoding="utf-8")
    PY

    cargo update --workspace --offline
    echo "Bumped version: ${CURRENT} -> ${NEXT}"

    if [[ "{{no_commit}}" != "true" ]]; then
        git add Cargo.toml
        if git ls-files --error-unmatch Cargo.lock >/dev/null 2>&1; then
            git add Cargo.lock
        fi
        git commit -S -m "${MSG}"
    fi

# Create a signed annotated git tag for the current Cargo.toml version
[arg('push', long='push', value='true', help='Also push HEAD and the new tag to origin')]
[arg('dry_run', long='dry-run', value='true', help='Show planned tag without creating or pushing it')]
tag push='false' dry_run='false':
    #!/usr/bin/env bash
    set -euo pipefail

    VERSION="$(just version)"
    TAG="v${VERSION}"
    MSG="chore💤: release warpd-rs ${TAG}"

    if git rev-parse -q --verify "refs/tags/${TAG}" >/dev/null; then
        echo "error: tag ${TAG} already exists locally" >&2
        exit 1
    fi

    if [[ "{{dry_run}}" == "true" ]]; then
        echo "[dry-run] would run: git tag -s \"${TAG}\" -m \"${MSG}\""
        if [[ "{{push}}" == "true" ]]; then
            echo "[dry-run] would run: git push origin HEAD \"${TAG}\""
        fi
        exit 0
    fi

    just _ensure-clean
    git tag -s "${TAG}" -m "${MSG}"
    echo "Created signed tag ${TAG}"

    if [[ "{{push}}" == "true" ]]; then
        just push-release
    fi

# Push the current branch and current version tag to origin
[arg('dry_run', long='dry-run', value='true', help='Show planned git push without pushing')]
push-release dry_run='false':
    #!/usr/bin/env bash
    set -euo pipefail

    VERSION="$(just version)"
    TAG="v${VERSION}"

    if [[ "{{dry_run}}" == "true" ]]; then
        echo "[dry-run] would run: git push origin HEAD \"${TAG}\""
        exit 0
    fi

    if ! git rev-parse -q --verify "refs/tags/${TAG}" >/dev/null; then
        echo "error: tag ${TAG} does not exist locally (run 'just tag' first)" >&2
        exit 1
    fi

    git push origin HEAD "${TAG}"

# Create a GitHub release for the current version with pre-built binaries in dist/
[arg('draft', long='draft', value='true', help='Save the GitHub release as a draft')]
[arg('prerelease', long='prerelease', value='true', help='Mark the GitHub release as a prerelease')]
[arg('dry_run', long='dry-run', value='true', help='Show planned gh release create command without executing')]
gh-release draft='false' prerelease='false' dry_run='false':
    #!/usr/bin/env bash
    set -euo pipefail

    VERSION="$(just version)"
    TAG="v${VERSION}"
    BIN_CV="./dist/warpd-rs-${TAG}"
    BIN_NOCV="./dist/warpd-rs-nocv-${TAG}"

    ARGS=(release create "${TAG}" "${BIN_CV}" "${BIN_NOCV}" --title "${TAG}" --generate-notes)
    if [[ "{{draft}}" == "true" ]]; then
        ARGS+=(--draft)
    fi
    if [[ "{{prerelease}}" == "true" ]]; then
        ARGS+=(--prerelease)
    fi

    if [[ "{{dry_run}}" == "true" ]]; then
        echo "[dry-run] would run: gh ${ARGS[*]}"
        exit 0
    fi

    gh auth status >/dev/null
    if ! git ls-remote --exit-code --tags origin "refs/tags/${TAG}" >/dev/null 2>&1; then
        echo "error: tag ${TAG} is not pushed to origin (run 'just push-release' first)" >&2
        exit 1
    fi

    for bin in "${BIN_CV}" "${BIN_NOCV}"; do
        if [[ ! -f "${bin}" ]]; then
            echo "error: missing release binary ${bin} (run 'just build-bins' first)" >&2
            exit 1
        fi
    done

    gh "${ARGS[@]}"

# End-to-end release pipeline: check -> bump -> build-bins -> tag -> push -> gh-release
[arg('patch', long='patch', value='true', help='Bump patch version (x.y.Z)')]
[arg('minor', long='minor', value='true', help='Bump minor version (x.Y.0)')]
[arg('major', long='major', value='true', help='Bump major version (X.0.0)')]
[arg('set_version', long='version', help='Set explicit SemVer version (e.g. 0.3.0)')]
[arg('draft', long='draft', value='true', help='Save the GitHub release as a draft')]
[arg('prerelease', long='prerelease', value='true', help='Mark the GitHub release as a prerelease')]
[arg('skip_tests', long='skip-tests', value='true', help='Skip cargo test pre-flight checks')]
[arg('dry_run', long='dry-run', value='true', help='Preview the full release pipeline without making changes')]
release patch='false' minor='false' major='false' set_version='' draft='false' prerelease='false' skip_tests='false' dry_run='false':
    #!/usr/bin/env bash
    set -euo pipefail

    CURRENT="$(just version)"
    NEXT="$(just _resolve-version "{{patch}}" "{{minor}}" "{{major}}" "{{set_version}}")"
    TAG="v${NEXT}"

    # Pre-flight checks before mutating anything
    if git rev-parse -q --verify "refs/tags/${TAG}" >/dev/null; then
        echo "error: tag ${TAG} already exists locally" >&2
        exit 1
    fi
    if git ls-remote --exit-code --tags origin "refs/tags/${TAG}" >/dev/null 2>&1; then
        echo "error: tag ${TAG} already exists on origin" >&2
        exit 1
    fi

    if [[ "{{dry_run}}" == "true" ]]; then
        echo "=== Release pipeline dry-run: ${CURRENT} -> ${NEXT} (${TAG}) ==="
        if [[ -n "$(git status --porcelain)" ]]; then
            echo "[dry-run] warning: git working tree is dirty (would fail without --dry-run)"
        fi
        if ! gh auth status >/dev/null 2>&1; then
            echo "[dry-run] warning: gh is not authenticated (would fail without --dry-run)"
        fi
        if [[ "{{skip_tests}}" != "true" ]]; then
            echo "[dry-run] 1. just check"
        else
            echo "[dry-run] 1. (skipped) just check"
        fi
        echo "[dry-run] 2. just bump --version ${NEXT}"
        echo "[dry-run] 3. just build-bins  -> dist/warpd-rs-${TAG}, dist/warpd-rs-nocv-${TAG}"
        echo "[dry-run] 4. just tag         -> git tag -s ${TAG} -m \"chore💤: release warpd-rs ${TAG}\""
        echo "[dry-run] 5. just push-release -> git push origin HEAD ${TAG}"
        GH_FLAGS=""
        [[ "{{draft}}" == "true" ]] && GH_FLAGS+=" --draft"
        [[ "{{prerelease}}" == "true" ]] && GH_FLAGS+=" --prerelease"
        echo "[dry-run] 6. just gh-release${GH_FLAGS} -> gh release create ${TAG} ..."
        exit 0
    fi

    just _ensure-clean
    gh auth status >/dev/null

    echo "=== Starting release pipeline: ${CURRENT} -> ${NEXT} (${TAG}) ==="

    if [[ "{{skip_tests}}" != "true" ]]; then
        echo "--> [1/6] Running tests..."
        just check
    else
        echo "--> [1/6] Skipping tests (--skip-tests)"
    fi

    echo "--> [2/6] Bumping version and committing..."
    just bump --version "${NEXT}"

    echo "--> [3/6] Building release binaries..."
    just build-bins

    echo "--> [4/6] Creating signed tag ${TAG}..."
    just tag

    echo "--> [5/6] Pushing commit and tag to origin..."
    just push-release

    echo "--> [6/6] Publishing GitHub release ${TAG}..."
    GH_ARGS=()
    if [[ "{{draft}}" == "true" ]]; then
        GH_ARGS+=(--draft)
    fi
    if [[ "{{prerelease}}" == "true" ]]; then
        GH_ARGS+=(--prerelease)
    fi
    just gh-release "${GH_ARGS[@]}"

    echo "=== Released warpd-rs ${TAG} ==="

[private]
_ensure-clean:
    #!/usr/bin/env bash
    set -euo pipefail
    if [[ -n "$(git status --porcelain)" ]]; then
        echo "error: git working tree is not clean; commit or stash changes before releasing" >&2
        git status --short >&2
        exit 1
    fi

[private]
_resolve-version patch minor major set_version:
    #!/usr/bin/env python3
    import re
    import sys
    import tomllib

    patch = "{{patch}}" == "true"
    minor = "{{minor}}" == "true"
    major = "{{major}}" == "true"
    explicit = "{{set_version}}".strip().lstrip("v")

    chosen = sum([patch, minor, major, bool(explicit)])
    if chosen == 0:
        sys.exit("error: specify one of --patch, --minor, --major, or --version <X.Y.Z>")
    if chosen > 1:
        sys.exit("error: specify only one of --patch, --minor, --major, or --version <X.Y.Z>")

    current = tomllib.load(open("Cargo.toml", "rb"))["package"]["version"]
    m = re.fullmatch(r"(\d+)\.(\d+)\.(\d+)(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?", current)
    if not m:
        sys.exit(f"error: current Cargo.toml version '{current}' is not valid SemVer")
    maj, min_, pat = map(int, m.groups())

    if explicit:
        if not re.fullmatch(r"\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?", explicit):
            sys.exit(f"error: invalid SemVer version '{explicit}'")
        if explicit == current:
            sys.exit(f"error: target version '{explicit}' is already the current version")
        next_ver = explicit
    elif major:
        next_ver = f"{maj + 1}.0.0"
    elif minor:
        next_ver = f"{maj}.{min_ + 1}.0"
    else:
        next_ver = f"{maj}.{min_}.{pat + 1}"

    print(next_ver)
