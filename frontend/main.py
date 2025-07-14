from nicegui import ui

from config.routes import setup_routes
from config.settings import Settings


def create_app():
    settings = Settings()

    setup_routes()

    ui.run(
        host=settings.host,
        port=settings.port,
        title=settings.app_name,
        dark=settings.dark_mode,
        reload=True,
        show=False
    )


if __name__ == "__main__":
    create_app()
