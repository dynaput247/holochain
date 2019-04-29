let
  audit = import ./src/audit.nix;
  deploy = import ./src/deploy.nix;
  prepare = import ./src/prepare.nix;
in
[
  audit
  deploy
  prepare
]
++ import ./docs/build.nix
++ import ./github/build.nix
++ import ./npm/build.nix
++ import ./pulse/build.nix
++ import ./rust/build.nix
