from nicegui import ui
from ..services.auth_service import AuthService


def show():
    """Login Page"""
    auth_service = AuthService()
    with ui.card().classes('w-96 mx-auto mt-20'):
        ui.label('Login').classes('text-2xl font-bold mb-4')

        with ui.column().classes('w-full gap-4'):
            username_input = ui.input('Username').classes('w-full')
            password_input = ui.input('Password', password=True).classes('w-full')

            login_button = ui.button('Login').classes('w-full')

            # Status-Anzeige
            status_label = ui.label('').classes('text-red-500 text-sm')

            async def handle_login():
                """Login-Handler"""
                username = username_input.value
                password = password_input.value

                if not username or not password:
                    status_label.text = 'Bitte Username und Password eingeben'
                    return

                # Loading-State
                login_button.text = 'Logging in...'
                login_button.disable()
                status_label.text = ''

                try:
                    # Login beim Backend
                    result = await auth_service.login(username, password)

                    if result['success']:
                        # Erfolg - redirect to home
                        ui.navigate.to('/dashboard')
                    else:
                        # Fehler anzeigen
                        status_label.text = result.get('error', 'Login fehlgeschlagen')

                except Exception as e:
                    status_label.text = f'Fehler: {str(e)}'

                finally:
                    login_button.text = 'Login'
                    login_button.enable()

            login_button.on('click', handle_login)

            password_input.on('keydown.enter', handle_login)
