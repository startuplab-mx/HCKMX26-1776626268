#!/usr/bin/env bash
# Setup one-time para que `cargo build` para iOS funcione con ort static link.
#
# Hace tres cosas (idempotente):
#   1. Genera .cargo/config.toml con paths absolutos para el build script
#      override de ort-sys.
#   2. Descarga onnxruntime.xcframework (~30 MB) si no está vendorado.
#   3. lipo -thin extrae el slice/arch para cada target iOS soportado y los
#      coloca en app/src-tauri/.ort_link/<target>/libonnxruntime.a.
#
# Por qué necesario: ort-sys verifica la existencia de libonnxruntime.a al
# compilar su lib (no al link final). Como su build.rs es saltado por el
# override, los archivos deben existir ANTES de que cargo arranque.
#
# Uso: bash scripts/setup.sh

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ORT_VERSION="1.22.0"

echo "==> 1. Generando .cargo/config.toml..."
mkdir -p "$ROOT/.cargo"
cat > "$ROOT/.cargo/config.toml" <<EOF
# Generado por scripts/setup.sh. NO editar a mano.

[env]
IPHONEOS_DEPLOYMENT_TARGET = "14.0"

# Build script override: salta build.rs de ort-sys, enlaza estáticamente
# contra libonnxruntime.a preparado por este setup script.

[target.aarch64-apple-ios.onnxruntime]
rustc-link-lib = ["static=onnxruntime", "framework=Foundation", "framework=CoreFoundation", "framework=CoreML"]
rustc-link-search = ["native=$ROOT/app/src-tauri/.ort_link/aarch64-apple-ios"]

[target.aarch64-apple-ios-sim.onnxruntime]
rustc-link-lib = ["static=onnxruntime", "framework=Foundation", "framework=CoreFoundation", "framework=CoreML"]
rustc-link-search = ["native=$ROOT/app/src-tauri/.ort_link/aarch64-apple-ios-sim"]

[target.aarch64-apple-ios]
rustflags = ["-C", "link-arg=-Wl,-rpath,@executable_path/Frameworks"]

[target.aarch64-apple-ios-sim]
rustflags = ["-C", "link-arg=-Wl,-rpath,@executable_path/Frameworks"]
EOF
echo "    ✓ .cargo/config.toml"

echo "==> 2. Verificando onnxruntime.xcframework..."
XCFW_ROOT="$ROOT/tauri-plugin-native-browser-pane/ios/vendor/onnxruntime.xcframework"
if [ ! -d "$XCFW_ROOT" ]; then
    echo "    Descargando $ORT_VERSION..."
    mkdir -p "$(dirname "$XCFW_ROOT")"
    TMP=$(mktemp -d)
    URL="https://download.onnxruntime.ai/pod-archive-onnxruntime-c-${ORT_VERSION}.zip"
    curl -fsSLo "$TMP/ort.zip" "$URL"
    unzip -q "$TMP/ort.zip" -d "$TMP"
    mv "$TMP/onnxruntime.xcframework" "$XCFW_ROOT"
    rm -rf "$TMP"
    echo "    ✓ vendoreado en $XCFW_ROOT"
else
    echo "    ✓ ya vendoreado"
fi

echo "==> 3. Preparando libonnxruntime.a por target..."
prep_target() {
    local target="$1"
    local slice="$2"
    local arch="$3"
    local src="$XCFW_ROOT/$slice/onnxruntime.framework/onnxruntime"
    local dst_dir="$ROOT/app/src-tauri/.ort_link/$target"
    local dst="$dst_dir/libonnxruntime.a"

    if [ ! -f "$src" ]; then
        echo "    ⚠ slice $slice no existe en xcframework — skip $target"
        return
    fi

    mkdir -p "$dst_dir"
    if [ ! -f "$dst" ] || [ "$src" -nt "$dst" ]; then
        rm -f "$dst"
        lipo -thin "$arch" -output "$dst" "$src"
        echo "    ✓ $target ($arch from $slice)"
        # Cargo's build script override no emite rerun-if-changed para este .a,
        # así que cargo no re-linkea cuando cambia. Invalidamos manualmente.
        (cd "$ROOT" && cargo clean -p ort-sys --target "$target" 2>/dev/null || true)
    else
        echo "    ✓ $target (cache hit)"
    fi
}

prep_target "aarch64-apple-ios"     "ios-arm64"                       "arm64"
prep_target "aarch64-apple-ios-sim" "ios-arm64_x86_64-simulator"      "arm64"

echo ""
echo "✓ Setup completo. Ahora puedes correr:"
echo "    cargo build --target aarch64-apple-ios-sim"
echo "    bun tauri ios dev"
