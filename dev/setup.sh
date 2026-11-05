#!/usr/bin/env sh
# Creates dev/config.toml (gitignored) for the local web deployment started by the Procfile:
# backend on :3001, CORS open for the dev web server. Safe to re-run, keeps an existing config.
set -eu
cd "$(dirname "$0")/.."
if [ ! -f dev/config.toml ]; then
  cargo run -q -p macworp_backend -- config \
    | sed -e 's/^port = .*/port = 3001/' \
          -e 's/^cors_allowed_origins = .*/cors_allowed_origins = ["*"]/' \
          -e 's#^projects_path = .*#projects_path = "dev_projects"#' \
    > dev/config.toml
  echo "wrote dev/config.toml"
fi
# Password of the default worker credential in the generated config, read by the Procfile worker
if [ ! -f dev/worker.password ]; then
  echo developer > dev/worker.password
  echo "wrote dev/worker.password"
fi
cat <<'EOF'
Next: `ultraman start` (db, backend, worker, web), then create users (prompts for a password), e.g. in a second terminal:
  cargo run -p macworp_backend -- user create alice Alice A alice@example.org
  cargo run -p macworp_backend -- user create bob Bob B bob@example.org
Web UI: http://127.0.0.1:5001 (log in in two browser profiles to try sharing).
EOF
