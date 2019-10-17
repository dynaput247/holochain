{ pkgs, config }:
{
 buildInputs = []

 ++ (pkgs.callPackage ./audit {
  pkgs = pkgs;
  config = config;
 }).buildInputs

  ++ (pkgs.callPackage ./versions {
  pkgs = pkgs;
  config = config;
 }).buildInputs

   ++ (pkgs.callPackage ./publish {
  pkgs = pkgs;
  config = config;
 }).buildInputs

 ++ (pkgs.callPackage ./github {
  pkgs = pkgs;
  config = config;
 }).buildInputs
 ;
}
