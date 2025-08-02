import os

from nicegui import app, ui
from .config.settings import Settings
from .config.routes import setup_routes


def create_app():
    settings = Settings()

    setup_routes()

    static_dir = os.path.abspath('projects')
    app.add_static_files('/projects', static_dir)

    ui.run(
        storage_secret="lol",
        host=settings.host,
        port=settings.port,
        title=settings.app_name,
        dark=settings.dark_mode,
        reload=True,
        show=False
    )


if __name__ in {"__main__", "__mp_main__"}:
    create_app()
