{
  description = "A terminal music player for TIDAL, Spotify, Apple Music, Deezer, YouTube Music, Subsonic and your own files";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];
      forAll = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
    in {
      packages = forAll (pkgs: rec {
        default = sonora-tui;
        sonora-tui = pkgs.rustPlatform.buildRustPackage {
          pname = "sonora-tui";
          version = (builtins.fromTOML (builtins.readFile ./Cargo.toml)).package.version;
          src = self;

          cargoLock = {
            lockFile = ./Cargo.lock;
            outputHashes = {
              "librespot-audio-0.8.0" = "sha256-M8zbz6WSXJnz6ESo1e9TZ6m3p9mUdQdCWYR1MlAl3gM=";
              "librespot-core-0.8.0" = "sha256-M8zbz6WSXJnz6ESo1e9TZ6m3p9mUdQdCWYR1MlAl3gM=";
              "librespot-metadata-0.8.0" = "sha256-M8zbz6WSXJnz6ESo1e9TZ6m3p9mUdQdCWYR1MlAl3gM=";
              "librespot-oauth-0.8.0" = "sha256-M8zbz6WSXJnz6ESo1e9TZ6m3p9mUdQdCWYR1MlAl3gM=";
              "librespot-playback-0.8.0" = "sha256-M8zbz6WSXJnz6ESo1e9TZ6m3p9mUdQdCWYR1MlAl3gM=";
              "librespot-protocol-0.8.0" = "sha256-M8zbz6WSXJnz6ESo1e9TZ6m3p9mUdQdCWYR1MlAl3gM=";
              "music-0.43.0" = "sha256-xbVK46IYhnkggey/+jvokZL2TIf1TNOJQJDmQo44Vrs=";
              "storage-0.43.0" = "sha256-xbVK46IYhnkggey/+jvokZL2TIf1TNOJQJDmQo44Vrs=";
              "widevine-0.43.0" = "sha256-xbVK46IYhnkggey/+jvokZL2TIf1TNOJQJDmQo44Vrs=";
              "rodio-0.22.2" = "sha256-9UocC1oWKBviaPsLWXvt53qBFasZonYArzThxIiJFSE=";
              "ytmusic-0.1.0" = "sha256-WsZleHDO/O7f8e5dtQpOtu+oToxx7POWdsOZ/3I5JHQ=";
            };
          };

          # cmake for the bundled Opus decoder and aws-lc, pkg-config for ALSA.
          nativeBuildInputs = [ pkgs.pkg-config pkgs.cmake ];
          buildInputs = pkgs.lib.optionals pkgs.stdenv.isLinux [ pkgs.alsa-lib ];

          # The tests drive a terminal and, on Linux, a D-Bus session.
          doCheck = false;

          meta = {
            description = "A terminal music player for TIDAL, Spotify, Apple Music, Deezer, YouTube Music, Subsonic and your own files";
            homepage = "https://github.com/ColinTenaguillo/sonora-tui";
            license = pkgs.lib.licenses.gpl3Plus;
            mainProgram = "sonora-tui";
            platforms = systems;
          };
        };
      });
    };
}
