# std imports
from io import TextIOWrapper, StringIO
import logging
from typing import Annotated
from pathlib import Path as FsPath
import json

# 3rd party imports
from fastapi import APIRouter, Depends, FastAPI, Path, Query, Request
import uvicorn


def get_dummy_log_file() -> TextIOWrapper:
    return StringIO()


class WeblogProxy:
    """
    Proxy for sending weblog requests to the NFCloud API using credentials.
    """

    @staticmethod
    async def nextflow(
        project_id: int,
        request: Request,
        log_file: TextIOWrapper = Depends(get_dummy_log_file),
    ):
        """
        Receives the weblogs from Nextflow and forwards them to the NFCloud API.

        Parameters
        ----------
        project_id : ints
            Project ID
        workflow_engine_type : str
            Workflow engine type, see: `macworp_utils.constants.SupportedWorkflowEngine`
        """
        print(project_id)
        print("body:", await request.body())

    @staticmethod
    async def snakemake_service_info(
        request: Request, log_file: TextIOWrapper = Depends(get_dummy_log_file)
    ):
        data = await request.body()
        entry = f"---\n[{request.method}] {request.url}"
        entry += f"\n{data.decode()}\n\n"
        log_file.write(entry)
        log_file.flush()
        return {
            "status": "running",
        }

    @staticmethod
    async def snakemake_create_workflow(
        project_id: Annotated[int | None, Query(gt=0)],
        request: Request,
        log_file: TextIOWrapper = Depends(get_dummy_log_file),
    ):
        data = await request.body()
        entry = f"---\n[{request.method}] {request.url}"
        entry += f"\n{data.decode()}\n\n"
        log_file.write(entry)
        log_file.flush()

        return {"id": project_id}

    @staticmethod
    async def snakemake_update_workflow_status(
        request: Request, log_file: TextIOWrapper = Depends(get_dummy_log_file)
    ):
        data = await request.form()
        entry = f"---\n[{request.method}] {request.url}"
        entry += f"\n{data.get('msg')}\n\n"
        log_file.write(entry)
        log_file.flush()

    @staticmethod
    async def snakemake_workflow(
        project_id: int,
        request: Request,
        log_file: TextIOWrapper = Depends(get_dummy_log_file),
    ):
        data = await request.body()
        entry = f"---\n[{request.method}] {request.url}"
        entry += f"\npid: {project_id}\n"
        entry += f"\n{data.decode()}\n\n"
        log_file.write(entry)
        log_file.flush()

    @staticmethod
    def get_router() -> APIRouter:
        router = APIRouter()

        router.add_api_route(
            "/nextflow/{project_id:int}",
            WeblogProxy.nextflow,
            methods=["POST"],
        )

        router.add_api_route(
            "/snakemake/api/service-info",
            WeblogProxy.snakemake_service_info,
            methods=["GET"],
        )

        router.add_api_route(
            "/snakemake/create_workflow",
            WeblogProxy.snakemake_create_workflow,
            methods=["GET"],
        )

        router.add_api_route(
            "/snakemake/update_workflow_status",
            WeblogProxy.snakemake_update_workflow_status,
            methods=["POST"],
        )

        router.add_api_route(
            "/snakemake/api/workflow/{project_id:int}",
            WeblogProxy.snakemake_workflow,
            methods=["PUT"],
        )

        return router

    @classmethod
    def server_process(cls, port: int, log_level: int):
        """
        Starts a FastAPI server to proxy weblog requests to the NFCloud API.
        Should be used in a separate process.s

        Parameters
        ----------
        settings : WeblogProxySettings
            Settings containing the NFCloud API URL and credentials
        port : int
            Port for the FastAPI server
        log_level : int
            Log level as defined in logging module
            If log level is DEBUG, access log will be enabled
        """
        log_file = FsPath("weblog_proxy.log").open("w", encoding="utf-8")

        def get_log_file() -> TextIOWrapper:
            return log_file

        app = FastAPI()
        app.dependency_overrides[get_dummy_log_file] = get_log_file
        app.include_router(cls.get_router())

        uvicorn.run(
            app,
            host="127.0.0.1",
            port=port,
            reload=False,
            log_level=log_level,
            access_log=log_level == logging.DEBUG,
        )


if __name__ == "__main__":
    WeblogProxy.server_process(9000, logging.DEBUG)
