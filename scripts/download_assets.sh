#!/usr/bin/env bash
set -euo pipefail

ASSETS_DIR="$HOME/.mythrax/models"
mkdir -p "$ASSETS_DIR"

echo "=== Downloading nomic-embed-text-v1.5 ONNX ==="
curl -L -o "$ASSETS_DIR/nomic-embed-text-v1.5.onnx" \
  "https://huggingface.co/nomic-ai/nomic-embed-text-v1.5/resolve/main/onnx/model.onnx"

echo "=== Downloading Tokenizer Configs ==="
curl -L -o "$ASSETS_DIR/tokenizer.json" \
  "https://huggingface.co/nomic-ai/nomic-embed-text-v1.5/resolve/main/tokenizer.json"
curl -L -o "$ASSETS_DIR/tokenizer_config.json" \
  "https://huggingface.co/nomic-ai/nomic-embed-text-v1.5/resolve/main/tokenizer_config.json"

echo "=== Installing SurrealDB binary (macOS / Linux) ==="
if [[ "$OSTYPE" == "darwin"* ]]; then
  brew install surrealdb/tap/surreal
else
  curl --proto '=https' --tlsv1.2 -sSf https://install.surreal.db | sh
fi
echo "Asset download completed successfully."
