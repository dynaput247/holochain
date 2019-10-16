{ pkgs }:
let
 name = "hc-test-fmt";

 # the reason this exists (instead of just hn-rust-fmt-check)
 # is to avoid things that aren't compatible with current version
 # of fmt
 # @todo rip this out when fmt catches up with nightly
 # @see https://github.com/rust-lang/rustfmt/issues/3666
 # @see https://github.com/rust-lang/rustfmt/issues/3685
 script = pkgs.writeShellScriptBin name
 ''
 echo "checking rust formatting"
 for p in \
  hc \
  holochain_common \
  holochain \
  holochain_conductor_lib \
  holochain_conductor_wasm \
  hdk \
  hdk-v2 \
  holochain_net \
  holochain_dpki \
  benchmarks
 do
  echo "checking ''${p}"
  cargo fmt -p $p -- --check
 done
 '';
in
{
 buildInputs = [ script ];
}
