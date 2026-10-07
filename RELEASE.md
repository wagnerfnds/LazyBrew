# Publishing a release

1. Bump the package version in Cargo.toml and regenerate Cargo.lock with Cargo.
2. Run the README checks; update previews and validation notes if behavior changed.
3. Commit and push, then wait for the Rust CI checks on macOS and Linux to pass.
4. Create and push a matching `vVERSION` tag. The Release binaries workflow builds,
   tests and packages native Apple Silicon and Intel binaries for macOS 13+.
5. Download the two workflow artifacts, verify their SHA-256 manifests, and create
   the GitHub release from that tag with both archives and checksum files attached.
6. Update the URLs and SHA-256 values in Formula/lazybrew.rb in
   https://github.com/wagnerfnds/homebrew-tap. Run brew style, brew audit and brew test,
   and verify an installation from the released archive before pushing the tap.

The workflow only creates build artifacts. Publication is a separate step, so a
failed build never creates a partially populated public release. Existing version
assets must not be replaced; corrections require a new release version.

Users install with `brew install wagnerfnds/tap/lazybrew` and receive updates through
`brew update` / `brew upgrade lazybrew`. The tap is maintained by this project and
is separate from the Homebrew/core catalogue.
