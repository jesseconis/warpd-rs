# Read version from Cargo.toml automatically
version := `cargo metadata --no-deps --format-version 1 | python3 -c "import sys,json; print(json.load(sys.stdin)['packages'][0]['version'])"`

build-bins:
    @echo "Building warpd-rs v{{version}}..."
    mkdir -p dist

    # With opencv (default features)
    cargo build --release
    cp target/release/warpd-rs dist/warpd-rs-v{{version}}

    # Without opencv
    cargo build --release --no-default-features
    cp target/release/warpd-rs dist/warpd-rs-nocv-v{{version}}

    @echo "Done:"
    @ls -lh dist/warpd-rs*v{{version}}

prepare-release level='patch':
    cargo-release release --execute {{level}} 

gh-release level='patch': (prepare-release level)
    #!/usr/bin/env bash
    just build-bins
    git fetch --tags
    VERSION=$(cargo metadata --no-deps --format-version 1 | python3 -c "import sys,json; print(json.load(sys.stdin)['packages'][0]['version'])")
    gh release create "v${VERSION}" \
        "./dist/warpd-rs-nocv-v${VERSION}" \
        "./dist/warpd-rs-v${VERSION}" \
        --generate-notes
