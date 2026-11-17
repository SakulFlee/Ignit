{
  description = "Ignit CLI";

  # Packaging only. There is intentionally no devShell here: CI uses plain
  # containers/rustup, and NixOS users can build projects with
  # `nix-shell -p cargo pkg-config systemd wayland libxkbcommon vulkan-loader mesa`.
  inputs.nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";

  outputs =
    { nixpkgs }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "aarch64-darwin"
      ];

      # Read the version from the workspace manifest so releases never have to
      # edit this file.
      version = (builtins.fromTOML (builtins.readFile ./Cargo.toml)).workspace.package.version;

      mkCli =
        pkgs:
        pkgs.rustPlatform.buildRustPackage {
          pname = "ignit-cli";
          inherit version;
          src = ./.;

          # `allowBuiltinFetchGit` avoids `outputHashes` for the git-sourced
          # engine dependencies: every one of them is pinned to a revision in
          # Cargo.lock. If a future nixpkgs drops that option, replace it with
          # an `outputHashes` entry per git dependency (the build error names
          # each package).
          cargoLock = {
            lockFile = ./Cargo.lock;
            allowBuiltinFetchGit = true;
          };

          # Package the CLI only; the workspace examples are not installed.
          # `cargoInstallHook` copies every binary produced in target/, so the
          # build flags double as the install filter.
          cargoBuildFlags = [ "-p" "ignit-cli" ];

          # The CLI tests spawn processes and are covered by `cargo test`.
          doCheck = false;

          meta = {
            description = "Scaffold, build and run Ignit projects";
            mainProgram = "ignit";
            license = with pkgs.lib.licenses; [
              mit
              asl20
            ];
            platforms = systems;
          };
        };
    in
    {
      packages = nixpkgs.lib.genAttrs systems (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
          ignit-cli = mkCli pkgs;
        in
        {
          inherit ignit-cli;
          default = ignit-cli;
        }
      );
    };
}
