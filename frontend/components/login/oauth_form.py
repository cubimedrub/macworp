from nicegui import ui


def create_oauth_form(provider, on_click):
    """Create OAuth login form"""
    with ui.column().classes('w-full') as container:
        with ui.card().classes('w-full p-6 text-center'):
            ui.label('OAuth Login').classes('text-lg font-semibold mb-4')
            ui.label('You will be redirected to the authentication provider').classes('text-sm text-gray-600 mb-6')

            ui.button(f'Login with {provider.title()}',
                      on_click=on_click
                      ).classes('w-full').props('color=primary size=lg')

            ui.icon('launch').classes('ml-2')

    return container
