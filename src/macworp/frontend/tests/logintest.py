import pytest
from nicegui import ui
from nicegui.testing import User

from macworp.frontend.main import setup_ui

pytest_plugins = ["nicegui.testing.user_plugin"]


async def test_provider_click(user: User) -> None:
    setup_ui()
    await user.open("/")
    await user.should_see("NF Cloud Login")
