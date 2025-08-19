from typing import Callable, Any
from nicegui import ui


def create_oauth_form(provider: str, on_click: Callable[[], Any]):
    """
    Create an OAuth login form UI.

    This form displays:
        - A title and description
        - A login button labeled with the OAuth provider name
        - An icon indicating a redirect

    Args:
        provider (str): The name of the OAuth provider.
        on_click (Callable[[], Any]): A callback function executed when
            the login button is clicked.

    Returns:
        Any: A NiceGUI column container (`ui.column`) that holds the form elements.
    """
    with ui.column().classes('w-full') as container:
        with ui.card().classes('w-full p-6 text-center'):
            ui.label('OAuth Login').classes('text-lg font-semibold mb-4')
            ui.label('You will be redirected to the authentication provider').classes('text-sm text-gray-600 mb-6')

            ui.button(f'Login with {provider.title()}',
                      on_click=on_click
                      ).classes('w-full').props('color=primary size=lg')

            ui.icon('launch').classes('ml-2')

    return container
