#!/usr/bin/env bash
# source https://github.com/zauberzeug/nicegui/blob/main/examples/fastapi/start.sh
# use path of this example as working directory; enables starting this script from anywhere

# Navigate to project root (one level up from frontend/)
cd "$(dirname "$0")/.."

# Add project root to Python path
export PYTHONPATH="${PYTHONPATH}:$(pwd)"

if [ "$1" = "prod" ]; then
    echo "Starting Uvicorn server in production mode..."
    # we also use a single worker in production mode so socket.io connections are always handled by the same worker
    uvicorn frontend.main:app --workers 1 --log-level info --port 80
elif [ "$1" = "dev" ]; then
    echo "Starting NiceGUI frontend in development mode..."
    # For NiceGUI frontend, run directly with Python
    python -m frontend.main
else
    echo "Invalid parameter. Use 'prod' or 'dev'."
    exit 1
fi