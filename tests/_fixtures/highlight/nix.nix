{ pkgs ? import <nixpkgs> { } }:

pkgs.mkShell {
  buildInputs = [ pkgs.rustc pkgs.cargo ];
  shellHook = "echo ready";
}
