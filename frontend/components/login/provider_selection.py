from nicegui import ui


def create_provider_selection(providers, on_select):
    """Erstellt die Provider-Auswahl in einer einzigen horizontalen Reihe, zentriert und mit begrenzter Breite"""
    with ui.row().classes('w-full justify-center '):
        with ui.card().classes('justify-center bg-white shadow-lg rounded-lg p-6 max-w-screen-md mx-auto '):
            ui.label('Choose Login Method')\
              .style('color: #6E93D6; font-size: 200%; font-weight: 300')\
              .classes('text-center mb-4')

            if not providers:
                ui.label('No login providers configured')\
                  .classes('text-center text-gray-500')
                return

            with ui.row().classes('items-center w-full flex flex-row flex-nowrap overflow-x-auto'):
                for provider_type, type_providers in providers.items():
                    for provider, description in type_providers.items():
                        create_provider_card(provider_type, provider, description, on_select)


def create_provider_card(provider_type, provider, description, on_select):
    """Erstellt eine einheitliche Provider-Karte"""
    with ui.card().classes('w-64 h-56 cursor-pointer hover:bg-gray-50 transition-colors flex flex-col justify-between p-4'):

        with ui.column().classes('gap-y-1'):
            ui.icon('account_circle').classes('text-3xl text-primary')
            ui.label(provider.title()).classes('text-lg font-semibold')
            ui.label(description).classes('text-sm text-gray-600')

        ui.button('Select', on_click=lambda: on_select(provider_type, provider)).classes('self-start')
