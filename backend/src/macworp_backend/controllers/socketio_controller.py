# 3rd party imports
from flask_socketio import join_room, leave_room

# internal imports
from macworp_backend import socketio, app
from macworp_backend.authorization import jwt


class SocketIoController:
    """
    Controller for global event handling, e.g. joining/leaving room
    """

    @staticmethod
    @socketio.on("join_project_updates")
    def on_join(data: dict):
        """
        Joining a room

        Parameters
        ----------
        data : dict
            Dict with key `room` and value room name
        """
        room = f"project{data['project_id']}"
        join_room(room)

    @staticmethod
    @socketio.on("leave_project_updates")
    def on_leave(data: dict):
        """
        Leave a room

        Parameters
        ----------
        data : dict
            Dict with key `room` and value room name
        """
        room = f"project{data['project_id']}"
        leave_room(room)
