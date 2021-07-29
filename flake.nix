{
  description = "dcompass project";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    utils.url = "github:numtide/flake-utils";
    naersk.url = "github:yusdacra/naersk/feat/cargolock-git-deps";
  };

  outputs = { nixpkgs, rust-overlay, utils, naersk, ... }:
    with nixpkgs.lib;
    let
      features = [ "geoip-maxmind" "geoip-cn" ];
      forEachFeature = f:
        builtins.listToAttrs (map (v:
          attrsets.nameValuePair "dcompass-${strings.removePrefix "geoip-" v}"
          (f v)) features);
      pkgSet = system:
        forEachFeature (v:
          naersk.lib."${system}".buildPackage {
            name = "dcompass-${strings.removePrefix "geoip-" v}";
            version = "git";
            root = ./.;
            passthru.exePath = "/bin/dcompass";
            nativeBuildInputs = with import nixpkgs { system = "${system}"; }; [
              # required for vendoring
              gnumake
              perl
            ];
            cargoBuildOptions = default:
              (default ++ [
                "--manifest-path ./dcompass/Cargo.toml"
                ''--features "${v}"''
              ]);
          });
    in utils.lib.eachSystem (utils.lib.defaultSystems) (system: rec {
      # `nix build`
      packages = (pkgSet system) // {
        # We have to do it like `nix develop .#commit` because libraries don't play well with `makeBinPath` or `makeLibraryPath`.
        commit = (import ./commit.nix {
          lib = utils.lib;
          pkgs = import nixpkgs {
            system = "${system}";
            overlays = [ rust-overlay.overlay ];
          };
        });
      };

      defaultPackage = packages.dcompass-maxmind;

      # We don't check packages.commit because techinically it is not a pacakge
      checks = builtins.removeAttrs packages [ "commit" ];

      # `nix run`
      apps = {
        update = utils.lib.mkApp {
          drv = with import nixpkgs { system = "${system}"; };
            pkgs.writeShellScriptBin "dcompass-update-data" ''
              set -e
              export PATH=${pkgs.lib.strings.makeBinPath [ wget gzip ]}
              wget -O ./data/full.mmdb --show-progress https://github.com/Dreamacro/maxmind-geoip/releases/latest/download/Country.mmdb
              wget -O ./data/cn.mmdb --show-progress https://github.com/Hackl0us/GeoIP2-CN/raw/release/Country.mmdb
              wget -O ./data/ipcn.txt --show-progress https://github.com/17mon/china_ip_list/raw/master/china_ip_list.txt
              gzip -f -k ./data/ipcn.txt
            '';
        };
      } // (forEachFeature (v:
        utils.lib.mkApp {
          drv = packages."dcompass-${strings.removePrefix "geoip-" v}";
        }));

      defaultApp = apps.dcompass-maxmind;

      # `nix develop`
      devShell = with import nixpkgs {
        system = "${system}";
        overlays = [ rust-overlay.overlay ];
      };
        mkShell {
          nativeBuildInputs = [
            # write rustfmt first to ensure we are using nightly rustfmt
            rust-bin.nightly."2021-01-01".rustfmt
            rust-bin.stable.latest.default
            rust-bin.stable.latest.rust-src
            rust-analyzer

            binutils-unwrapped
            cargo-cache

            perl
            gnumake
          ];
        };
    }) // {
      # public key for dcompass.cachix.org
      publicKey =
        "dcompass.cachix.org-1:uajJEJ1U9uy/y260jBIGgDwlyLqfL1sD5yaV/uWVlbk=";

      overlay = final: prev: {
        dcompass = recurseIntoAttrs (pkgSet naersk.lib."${prev.pkgs.system}");
      };
    };
}
