let
  rust_overlay = import (builtins.fetchTarball "https://github.com/oxalica/rust-overlay/archive/master.tar.gz");
  pkgs = import <nixpkgs> { overlays = [ rust_overlay ]; };
in
pkgs.mkShell {
  nativeBuildInputs = with pkgs; [
    # Gets you the latest stable rustc, cargo, rustfmt, clippy, and rust-analyzer
    rust-bin.stable.latest.default
  ];

  RUST_BACKTRACE = 1;
  SAUER_PACKET_LOG = "*, -N_POS, -N_CONNECT, -N_PING, -N_PONG, -N_CLIENTPING, -N_SERVINFO, -N_SERVMSG, -N_EDITENT, -N_CLIPBOARD, -N_PASTE, -N_EDITVAR";
}
