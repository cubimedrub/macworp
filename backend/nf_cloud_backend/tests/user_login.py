class UserLogin:
    """
    A pair of login id + password.
    """

    login_id: str
    password: str

    def __init__(self, login_id: str, password: str):
        self.login_id = login_id
        self.password = password