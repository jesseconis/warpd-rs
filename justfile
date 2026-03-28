# Read version from Cargo.toml automatically
version := `cargo metadata --no-deps --format-version 1 | python3 -c "import sys,json; print(json.load(sys.stdin)['packages'][0]['version'])"`

# Build both variants and place them in ./dist/
release:
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
