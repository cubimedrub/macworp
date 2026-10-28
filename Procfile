# Local web deployment: Postgres, backend on :3001, web UI on :5001.
# Run `dev/setup.sh` once, then `ultraman start` (cargo install ultraman; overmind/foreman work too).
db: docker compose up db
backend: sleep 3 && cargo run -p macworp_backend -- db-migrate dev/config.toml && cargo run -p macworp_backend -- serve dev/config.toml
web: MACWORP_BASE_URL=http://127.0.0.1:3001 dx serve --package macworp_web --web --port 5001
