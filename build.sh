#!/usr/bin/env bash
set -e

UNREAL_CONTENT="unreal-plugin/Omnigenesys/Content/MapGen"
UNITY_STREAMING="unity-package/StreamingAssets~/MapGen"
# GODOT_CONTENT="godot-addon/bin"

usage() {
    echo "Uso: ./build.sh [windows|linux|mac|all]"
    echo "  windows  Build para Windows x64 (padrão)"
    echo "  linux    Build para Linux x64"
    echo "  mac      Build para macOS x64"
    echo "  all      Build para todas as plataformas"
    exit 1
}

dist_windows() {
    echo "Building mapgen para Windows..."
    GOOS=windows GOARCH=amd64 go build -o mapgen.exe ./cmd/mapgen
    mkdir -p "$UNREAL_CONTENT"
    cp mapgen.exe "$UNREAL_CONTENT/mapgen.exe"
    mkdir -p "$UNITY_STREAMING"
    cp mapgen.exe "$UNITY_STREAMING/mapgen.exe"
    echo "  -> $UNREAL_CONTENT/mapgen.exe"
    echo "  -> $UNITY_STREAMING/mapgen.exe"
}

dist_linux() {
    echo "Building mapgen para Linux..."
    GOOS=linux GOARCH=amd64 go build -o mapgen ./cmd/mapgen
    mkdir -p "$UNREAL_CONTENT"
    cp mapgen "$UNREAL_CONTENT/mapgen"
    mkdir -p "$UNITY_STREAMING"
    cp mapgen "$UNITY_STREAMING/mapgen"
    echo "  -> $UNREAL_CONTENT/mapgen"
    echo "  -> $UNITY_STREAMING/mapgen"
}

dist_mac() {
    echo "Building mapgen para macOS..."
    GOOS=darwin GOARCH=amd64 go build -o mapgen_mac ./cmd/mapgen
    mkdir -p "$UNREAL_CONTENT"
    cp mapgen_mac "$UNREAL_CONTENT/mapgen_mac"
    mkdir -p "$UNITY_STREAMING"
    cp mapgen_mac "$UNITY_STREAMING/mapgen_mac"
    echo "  -> $UNREAL_CONTENT/mapgen_mac"
    echo "  -> $UNITY_STREAMING/mapgen_mac"
}

TARGET="${1:-windows}"

case "$TARGET" in
    windows) dist_windows ;;
    linux)   dist_linux ;;
    mac)     dist_mac ;;
    all)     dist_windows; dist_linux; dist_mac ;;
    *)       usage ;;
esac

echo "Pronto."
