# Local web deployment: Postgres, backend on :3001, one remote worker, web UI on :5001.
# Run `dev/setup.sh` once, then `ultraman start` (cargo install ultraman; overmind/foreman work too).
db: docker compose up db
backend: sleep 3 && cargo run -p macworp_backend -- db-migrate dev/config.toml && cargo run -p macworp_backend -- serve dev/config.toml
# Remote worker over HTTP (needs podman/docker and the macworp-nextflow:latest image, see docker/engine-nextflow.dockerfile).
# It retries until the backend is up. Shares dev_projects/ with the backend (same absolute path).
worker: cargo run -p macworp_worker -- -vv --backend-url http://127.0.0.1:3001 --password-file dev/worker.password --name dev-worker run
web: MACWORP_BASE_URL=http://127.0.0.1:3001 dx serve --package macworp_web --web --port 5001
