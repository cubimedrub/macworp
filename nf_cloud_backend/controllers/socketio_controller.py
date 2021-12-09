from flask_socketio import join_room, leave_room
from nf_cloud_backend import socketio, app

class SocketIoController:
    """
    Controller for global event handling, e.g. joining/leaving room
    """

    @staticmethod
    @socketio.on("join")
    def on_join(data: dict):
        """
        Joining a room

        Parameters
        ----------
        data : dict
            Dict with key `room` and value room name
        """
        room = data["room"]
        join_room(room)

    @staticmethod
    @socketio.on("leave")
    def on_leave(data: dict):
        """
        Leave a room

        Parameters
        ----------
        data : dict
            Dict with key `room` and value room name
        """
        room = data["room"]
        leave_room(room)