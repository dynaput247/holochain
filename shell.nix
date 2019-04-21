let

  pkgs = import ./holonix/nixpkgs/nixpkgs.nix;
  rust = import ./holonix/rust/config.nix;
  release = import ./holonix/release/config.nix;
  git = import ./holonix/git/config.nix;

  hc-prepare-crate-versions = pkgs.writeShellScriptBin "hc-prepare-crate-versions"
  ''
   echo "bumping core version from ${release.core.version.previous} to ${release.core.version.current} in Cargo.toml"
   find . \
    -name "Cargo.toml" \
    -not -path "**/.git/**" \
    -not -path "**/.cargo/**" \
    -not -path "./nodejs_conductor*" \
    | xargs -I {} \
    sed -i 's/^\s*version\s*=\s*"${release.core.version.previous}"\s*$/version = "${release.core.version.current}"/g' {}

   echo "bumping core versions from ${release.core.version.previous} to ${release.core.version.current} in readmes"
   find . \
    -iname "readme.md" \
    -not -path "**/.git/**" \
    -not -path "**/.cargo/**" \
    | xargs -I {} \
    sed -i 's/${release.core.version.previous}/${release.core.version.current}/g' {}

   echo "bumping versions from ${release.core.version.previous} to ${release.core.version.current} in CLI"
   find . \
    -type f \
    -not -path "**/.git/**" \
    -path "./cli/*" \
    | xargs -I {} \
    sed -i 's/${release.core.version.previous}/${release.core.version.current}/g' {}

   echo "bumping node conductor version from ${release.node-conductor.version.previous} to ${release.node-conductor.version.current}"
   sed -i 's/^\s*version\s*=\s*"${release.node-conductor.version.previous}"\s*$/version = "${release.node-conductor.version.current}"/g' ./nodejs_conductor/native/Cargo.toml
   sed -i 's/"version": "${release.node-conductor.version.previous}"/"version": "${release.node-conductor.version.current}"/g' ./nodejs_conductor/package.json
   sed -i 's/"@holochain\/holochain-nodejs": "${release.node-conductor.version.previous}"/"@holochain\/holochain-nodejs": "${release.node-conductor.version.current}"/g' ./cli/src/cli/js-tests-scaffold/package.json
  '';

in
with pkgs;
stdenv.mkDerivation rec {
  name = "holochain-rust-environment";

  buildInputs = [

    # I forgot what these are for!
    # Reinstate and organise them ᕙ༼*◕_◕*༽ᕤ
    # coreutils

    hc-prepare-crate-versions

  ]

  # root build inputs
  ++ import ./holonix/build.nix;

  # https://github.com/rust-unofficial/patterns/blob/master/anti_patterns/deny-warnings.md
  # https://llogiq.github.io/2017/06/01/perf-pitfalls.html
  RUSTFLAGS = "-D warnings -Z external-macro-backtrace -Z thinlto -C codegen-units=16 -C opt-level=z";
  CARGO_INCREMENTAL = "1";
  # https://github.com/rust-lang/cargo/issues/4961#issuecomment-359189913
  # RUST_LOG = "info";

  # non-nixos OS can have a "dirty" setup with rustup installed for the current
  # user.
  # `nix-shell` can inherit this e.g. through sourcing `.bashrc`.
  # even `nix-shell --pure` will still source some files and inherit paths.
  # for those users we can at least give the OS a clue that we want our pinned
  # rust version through this environment variable.
  # https://github.com/rust-lang/rustup.rs#environment-variables
  # https://github.com/NixOS/nix/issues/903
  RUSTUP_TOOLCHAIN = "nightly-${rust.nightly-date}";

  DARWIN_NIX_LDFLAGS = if stdenv.isDarwin then "-F${frameworks.CoreFoundation}/Library/Frameworks -framework CoreFoundation " else "";

  OPENSSL_STATIC = "1";

  shellHook = ''
   # cargo installs things to the user's home so we need it on the path
   export PATH=$PATH:~/.cargo/bin
   export HC_TARGET_PREFIX=~/nix-holochain/
   export NIX_LDFLAGS="$DARWIN_NIX_LDFLAGS$NIX_LDFLAGS"
  '';
}
