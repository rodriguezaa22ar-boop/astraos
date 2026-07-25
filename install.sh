\
#!/usr/bin/env bash
set -Eeuo pipefail

[[ "$(uname -s)" == "Darwin" ]] || {
  echo "AstraOS currently supports macOS only." >&2
  exit 1
}

command -v brew >/dev/null 2>&1 || {
  echo "Homebrew is required." >&2
  exit 1
}

command -v cargo >/dev/null 2>&1 || brew install rustup-init

if ! command -v cargo >/dev/null 2>&1; then
  rustup-init -y
  source "$HOME/.cargo/env"
fi

cargo build --release

mkdir -p "$HOME/.local/bin"
cp target/release/astra "$HOME/.local/bin/astra"
chmod +x "$HOME/.local/bin/astra"

touch "$HOME/.zshrc"
grep -Fq 'export PATH="$HOME/.local/bin:$PATH"' "$HOME/.zshrc" || \
  printf '\nexport PATH="$HOME/.local/bin:$PATH"\n' >> "$HOME/.zshrc"

echo
echo "Installed AstraOS CLI."
echo "Open a new terminal or run:"
echo '  export PATH="$HOME/.local/bin:$PATH"'
echo
echo "Then:"
echo "  astra dashboard"
echo "  astra doctor"
