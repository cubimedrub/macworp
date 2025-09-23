from fastapi import APIRouter, Response


router = APIRouter(prefix="/api/healthcheck")


@router.get("/ping", summary="returns pong for healthcheck")
def ping():
    """
    Returns pong for healthcheck
    """
    return Response(
        content="pong",
        media_type="text/plain",
        status_code=200,
    )
