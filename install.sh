#!/bin/bash

set -e

BINARY_NAME="clipboard-sync-lite"

if [ -n "$INSTALL_DIR" ]; then
    BIN_DIR="$INSTALL_DIR"
elif [ -n "$XDG_BIN_HOME" ]; then
    BIN_DIR="$XDG_BIN_HOME"
elif [ -n "$XDG_DATA_HOME" ]; then
    BIN_DIR="$XDG_DATA_HOME/../bin"
elif [ -d "$HOME/.local/bin" ]; then
    BIN_DIR="$HOME/.local/bin"
else
    BIN_DIR="$HOME/bin"
fi

cargo build --release

mkdir -p "$BIN_DIR"

cp "target/release/$BINARY_NAME" "$BIN_DIR/"

chmod +x "$BIN_DIR/$BINARY_NAME"


if [[ ":$PATH:" != *":$BIN_DIR:"* ]]; then
    echo ""
    echo "Warning: $BIN_DIR is not in your PATH."
fi
