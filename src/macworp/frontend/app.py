import os
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

    setup_routes(config)

    static_dir = os.path.abspath("projects")
    app.add_static_files("/projects", static_dir)

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
