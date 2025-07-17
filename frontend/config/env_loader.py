import os
from dotenv import load_dotenv
from pathlib import Path

def load_environment():
    env_file = os.getenv("ENV_FILE", "dev.env")
    env_path = Path(__file__).resolve().parent.parent.parent / env_file

    if env_path.exists():
        load_dotenv(dotenv_path=env_path)
    else:
        print(f"[WARN] Keine Umgebungsdatei gefunden unter: {env_path}")
