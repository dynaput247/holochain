{ pkgs, config }:
{
 buildInputs = []

 ++ (pkgs.callPackage ./audit {
  pkgs = pkgs;
  config = config;
 }).buildInputs

 ++ (pkgs.callPackage ./github {
  pkgs = pkgs;
  config = config;
 }).buildInputs
 ;
}
