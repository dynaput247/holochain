{ pkgs }:
let
  config = import ./nix/config.nix;
  github = import ./github;

  pulse = pkgs.callPackage ./pulse {
   release = config;
   github = github;
  };

  rust = pkgs.callPackage ./rust {
   release = config;
  };
in
{
 buildInputs = [
  (pkgs.callPackage ./nix/audit.nix {
   config = config;
   pulse = pulse;
   })

   (pkgs.callPackage ./nix/branch.nix {
    release = config;
    github = github;
   })
 ]
 ++ pulse.buildInputs
 ++ rust.buildInputs
 ;

 config = config;
}
