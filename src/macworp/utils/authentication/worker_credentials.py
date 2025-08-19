from base64 import b64encode
from hashlib import sha256


def hash_worker_credentials(username: str, password: str) -> str:
    """
    Hashes the worker credentials using SHA-256. This is used to securly send the credentials

    Parameters
    ----------
    username : str
        The username of the worker.
    password : str
        The password of the worker.

    Returns
    -------
    str
        The SHA-256 base64 encoded hash of the worker credentials in the format "username:password".
    """
    return b64encode(sha256(f"{username}:{password}".encode("utf-8")).digest()).decode(
        "utf-8"
    )
