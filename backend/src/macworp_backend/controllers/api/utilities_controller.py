from pathlib import Path

from macworp_backend import app
from macworp_backend.utility.configuration import Configuration


class UtilityController:
    """
    Controller for various utility endpoints like healthchecks.
    """

    @staticmethod
    @app.route("/api/ping")
    def ping():
        """
        Endpoint for checking if app is running.

        Returns
        -------
        Response
        """
        return "pong", 200


    @staticmethod
    @app.route("/api/utilities/exec-uuid")
    def exec_uuid():
        """
        Returns the UUID

        Returns
        -------
        Response
        """
        uuid: str = Path(Configuration.values()["upload_path"]).joinpath("exec-uuid.txt").read_text().strip()

        return uuid, 200
