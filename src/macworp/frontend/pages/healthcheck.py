from nicegui import ui


class Healthcheck:

    @classmethod
    async def ping(cls):
        ui.label("pong")
