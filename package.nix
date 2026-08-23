{ lib, rustPlatform }:

rustPlatform.buildRustPackage rec {
  pname = "wand2c";
  version = "2.0.0";

  src = ./.;

  cargoLock = {
    lockFile = ./Cargo.lock;
  };

  meta = with lib; {
    description = "A native compiler for the WandC v2.0 systems programming language";
    license = licenses.mit;
    platforms = platforms.linux;
    mainProgram = "wand2c";
  };
}
