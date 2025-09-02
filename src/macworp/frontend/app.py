import os
from pathlib import Path
from typing import Optional

from nicegui import app, ui

from macworp.configuration import Configuration
from macworp.frontend.config.routes import setup_routes


def start_app(config: Configuration):
    """
    Starts frontend application

    Parameters
    ----------
    config : Configuration
        MAcWorP Configuration
    """

    config.projects_path.mkdir(parents=True, exist_ok=True)

    setup_routes(config)

    static_image_dir = Path(__file__).parent / "static"
    app.add_static_files("/projects", config.projects_path.absolute())
    app.add_static_files("/images", static_image_dir)

    ui.run(
        storage_secret=config.secret,
        host=config.frontend.host,
        port=config.frontend.port,
        title=config.frontend.app_name,
        dark=config.frontend.dark_mode,
        reload=config.development,
        uvicorn_reload_dirs=",".join([str(Path(__file__).parent.parent.as_posix())]),
        show=False,
    )
