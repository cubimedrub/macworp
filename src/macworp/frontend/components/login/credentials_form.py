from typing import Callable, Any

from nicegui import ui


def create_credentials_form(on_submit: Callable[[str, str], Any]) -> dict:
    """
    Create username/password login form UI

    Args:
        on_submit (Callable[[str,str], Any]): A callback function that will be
            called when the login form is submitted, either by pressing "Enter"
            in the password field or by clicking the login button.
            Receives the entered username and password as arguments.

    Returns:
        dict: A dictionary containing references to the UI elements:
            - 'container': Column container holding the form
            - 'username_input': Username input element
            - 'password_input': Password input element
            - 'login_button': Login button element
            - 'loading_spinner': Spinner element for loading indication

    """
    form_container = ui.column().classes('w-full')

    with form_container:
        with ui.card().classes('w-full p-6'):
            ui.label('Enter your credentials').classes('text-lg font-semibold mb-4')

            username_input = ui.input('Username', placeholder='Enter username'
                                      ).classes('w-full').props('outlined')

            password_input = ui.input('Password', placeholder='Enter password', password=True
                                      ).classes('w-full').props('outlined')

            password_input.on('keydown.enter', lambda: on_submit(username_input.value, password_input.value))

            with ui.row().classes('w-full mt-4 space-x-2'):
                login_button = ui.button('Login',
                                         on_click=lambda: on_submit(username_input.value, password_input.value)
                                         ).classes('flex-1').props('color=primary')

                loading_spinner = ui.spinner(size='sm').classes('ml-2')
                loading_spinner.set_visibility(False)

    return {
        'container': form_container,
        'username_input': username_input,
        'password_input': password_input,
        'login_button': login_button,
        'loading_spinner': loading_spinner
    }
