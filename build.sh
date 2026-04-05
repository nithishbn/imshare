#!/usr/bin/env bash
#
# Build script for imshare (NixOS)
#

set -e

echo "Building imshare with Nix..."
echo ""
echo "Option 1: Build with cargo (in nix develop shell)"
echo "  nix develop"
echo "  cargo build --release"
echo ""
echo "Option 2: Build with nix build"
echo "  nix build"
echo ""

read -p "Choose option (1 or 2): " choice

case $choice in
    1)
        echo "Entering nix develop shell..."
        nix develop -c cargo build --release
        echo ""
        echo "Build complete! Binaries are in ./target/release/"
        echo "  - imshare"
        echo "  - imshare-verify"
        ;;
    2)
        echo "Building with nix..."
        nix build
        echo ""
        echo "Build complete! Binaries are in ./result/bin/"
        echo "  - imshare"
        echo "  - imshare-verify"
        ;;
    *)
        echo "Invalid choice. Please run again and choose 1 or 2."
        exit 1
        ;;
esac

echo ""
echo "Next steps:"
echo "  1. Set up your environment: export IMSHARE_SECRET=\$(openssl rand -base64 32)"
echo "  2. Copy config: cp config.toml.example ~/.config/imshare/config.toml"
echo "  3. Generate a link: ./target/release/imshare generate <uuid> --ttl 7d"
echo "  4. See README.md for complete setup instructions"
