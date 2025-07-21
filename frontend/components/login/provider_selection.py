from nicegui import ui


def create_provider_selection(providers, on_select):
    """Erstellt die Provider-Auswahl ohne Abhängigkeit zur LoginPage"""
    with ui.column().classes('w-full max-w-2xl mx-auto space-y-4') as container:
        ui.label('Choose Login Method').style('color: #6E93D6; font-size: 200%; font-weight: 300')

        if not providers:
            ui.label('No login providers configured').classes('text-center text-gray-500')
            return container

        for provider_type, type_providers in providers.items():
            if type_providers:
                ui.label(f"{provider_type.title()} Providers").classes('text-lg font-semibold mt-6 mb-2')

                for provider, description in type_providers.items():
                    create_provider_card(provider_type, provider, description, on_select)

        return container


def create_provider_card(provider_type, provider, description, on_select):
    """Erstellt eine einzelne Provider-Karte"""
    with ui.card().classes('cursor-pointer hover:bg-gray-50 transition-colors'):
        with ui.row().classes('items-center'):
            ui.icon('account_circle').classes('text-3xl text-primary')
            with ui.column():
                ui.label(provider.title()).classes('text-lg font-semibold')
                ui.label(description).classes('text-gray-600')

        ui.button('Select', on_click=lambda: on_select(provider_type, provider))
