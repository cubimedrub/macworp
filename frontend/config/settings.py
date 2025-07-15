from pydantic_settings import BaseSettings


class Settings(BaseSettings):
    # App-Einstellungen
    app_name: str = "NF-Cloud"
    debug: bool = False
    host: str = "0.0.0.0"
    port: int = 8080

    # Backend-Verbindung
    backend_url: str = "http://localhost:8000"
    api_version: str = "v1"

    # UI-Einstellungen
    dark_mode: bool = True
    theme_color: str = "#1976d2"

    # Auth-Einstellungen
    session_timeout: int = 3600

    class Config:
        env_file = ".env"
