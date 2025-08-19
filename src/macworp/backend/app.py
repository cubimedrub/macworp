from logging import DEBUG, INFO

from fastapi import FastAPI, Request
from fastapi.middleware.cors import CORSMiddleware
import uvicorn

from macworp.backend.database import init_database
from macworp.configuration import Configuration
from macworp.backend.controllers.depends import get_configuration
from macworp.backend.controllers import users
from macworp.backend.controllers import project
from macworp.backend.controllers import workflow


def get_app(config: Configuration) -> FastAPI:
    """
    Initializes the database connection and returns a FastAPI application instance ready to use.
    """

    init_database(config.backend.database)

    app = FastAPI()

    app.include_router(project.router)
    app.include_router(workflow.router)
    app.include_router(users.router)

    def get_given_configuration():
        """Function to return the configuration passed to the app via CLI"""
        return config

    # Because the settings coming via CLI and getting passed through the executor etc.
    # passing the setting as described in the FastAPI documentation is not possible.
    # Every path that uses the settings need an argument
    # `settings: Settings = Depends(get_dummy_settings)`
    # which is ultimately overridden by the `get_settings` function.
    app.dependency_overrides[get_configuration] = get_given_configuration

    app.add_middleware(
        CORSMiddleware,
        allow_origins=config.backend.allowed_origins,
        allow_credentials=True,
        allow_methods=["*"],
        allow_headers=["*"],
    )

    @app.middleware("http")
    async def add_process_time_header(request: Request, call_next):
        print(f"Request:\n\t{request.url}\n\t{request.method}\n\t{request.headers}")
        response = await call_next(request)
        return response

    return app


def start_app(config: Configuration):
    """Starts the FastAPI application with the given configuration."""

    app = get_app(config)

    log_level = DEBUG if config.debug else INFO

    uvicorn.run(
        app,
        host=config.backend.interface,
        port=config.backend.port,
        reload=False,
        log_level=log_level,
        access_log=log_level == DEBUG,
    )
