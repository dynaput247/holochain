let
  pkgs = import ./nixpkgs/nixpkgs.nix;

  flush = import ./src/flush.nix;
  test = import ./src/test.nix;

  release = import ./release/config.nix;
in
[
  # I forgot what these are for!
  # Reinstate and organise them ᕙ༼*◕_◕*༽ᕤ
  # coreutils

  flush
  test

]

++ import ./app-spec/build.nix
++ import ./app-spec-cluster/build.nix
++ import ./cli/build.nix
++ import ./conductor/build.nix
++ import ./darwin/build.nix
++ import ./dist/build.nix
++ import ./git/build.nix
++ import ./node/build.nix
++ import ./openssl/build.nix
++ import ./qt/build.nix
++ import ./release/build.nix
++ import ./rust/build.nix
