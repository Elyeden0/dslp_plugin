#!/bin/sh
# Installs the dslp binary matching this plugin version into $CLAUDE_PLUGIN_DATA:
# a prebuilt release download, else a background build from the bundled source.
# Stdout of a SessionStart hook is added to Claude's context; stay silent when dslp is ready.
version=$(sed -n 's/.*"version": *"\([^"]*\)".*/\1/p' "$CLAUDE_PLUGIN_ROOT/.claude-plugin/plugin.json")
dir="$CLAUDE_PLUGIN_DATA/bin"
bin="$dir/dslp"
mkdir -p "$dir"
# bin/dslp (on the Bash tool's PATH) execs the binary through this link.
ln -sfn "$bin" "$CLAUDE_PLUGIN_ROOT/bin/.dslp"

[ "$("$bin" --version 2>/dev/null)" = "dslp $version" ] && exit 0

case "$(uname -s)-$(uname -m)" in
  Linux-x86_64) target=x86_64-unknown-linux-musl ;;
  Linux-aarch64 | Linux-arm64) target=aarch64-unknown-linux-musl ;;
  Darwin-x86_64) target=x86_64-apple-darwin ;;
  Darwin-arm64) target=aarch64-apple-darwin ;;
  *) target= ;;
esac

if [ -n "$target" ]; then
  url="https://github.com/Elyeden0/dslp_plugin/releases/download/v$version/dslp-$target"
  if curl -fsL --max-time 120 -o "$bin.tmp" "$url" && chmod +x "$bin.tmp" &&
    [ "$("$bin.tmp" --version 2>/dev/null)" = "dslp $version" ]; then
    mv -f "$bin.tmp" "$bin"
    exit 0
  fi
  rm -f "$bin.tmp"
fi

log="$CLAUDE_PLUGIN_DATA/build.log"
if command -v cargo >/dev/null 2>&1; then
  # Concurrent sessions are fine: cargo serialises builds on the target-dir lock.
  (CARGO_TARGET_DIR="$CLAUDE_PLUGIN_DATA/target" cargo install --locked --force \
    --path "$CLAUDE_PLUGIN_ROOT/dslp-cli" --root "$CLAUDE_PLUGIN_DATA" \
    >"$log" 2>&1 </dev/null &)
  echo "No prebuilt dslp $version could be downloaded, so the dslp plugin is building it from source in the background (log: $log). dslp commands fail until it finishes, usually in a few minutes."
  exit 0
fi

cat <<MSG
The dslp plugin could not install dslp $version: no prebuilt binary could be downloaded for $(uname -s)-$(uname -m), and there is no Rust toolchain to build it.
Install Rust from https://rustup.rs and start a new session; the plugin then builds dslp itself.
MSG
