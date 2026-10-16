#!/bin/sh
# Stdout of a SessionStart hook is added to Claude's context; stay silent when dslp is installed.
command -v dslp >/dev/null 2>&1 && exit 0
cat <<'MSG'
The dslp plugin is enabled but the `dslp` binary is not on PATH, so dslp commands will fail.
Install it with:
  git clone https://github.com/Elyeden0/dslp_plugin.git && cargo install --path dslp_plugin/dslp-cli
then make sure ~/.cargo/bin is on PATH.
MSG
