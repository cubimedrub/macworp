from macworp_backend import app


class PingController:
    """
    Controller for ping endpoints.
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
