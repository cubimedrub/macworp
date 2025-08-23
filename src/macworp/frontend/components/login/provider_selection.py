from nicegui import ui
from typing import Callable, Dict


def create_provider_selection(providers: Dict[str, Dict[str, str]], on_select: Callable[[str, str], None]) -> None:
    """
    Create a provider selection UI in a single horizontal row.

    Args:
        providers (dict[str, dict[str, str]]): A dictionary where keys are
            provider types (e.g., "oauth", "saml") and values are dictionaries
            mapping provider names to descriptions.
        on_select (Callable[[str, str], None]): Callback executed when a provider
            is selected. Receives the provider type and provider name as arguments.

    Returns:
        None: This function creates UI elements but does not return a value.
    """
    with ui.row().classes('w-full justify-center '):
        with ui.card().classes('justify-center bg-white shadow-lg rounded-lg p-6 max-w-screen-md mx-auto '):
            ui.label('Choose Login Method') \
                .style('color: #6E93D6; font-size: 200%; font-weight: 300') \
                .classes('text-center mb-4')

            if not providers:
                ui.label('No login providers configured') \
                    .classes('text-center text-gray-500')
                return

            with ui.row().classes('items-center w-full flex flex-row flex-nowrap overflow-x-auto'):
                for provider_type, type_providers in providers.items():
                    for provider, description in type_providers.items():
                        create_provider_card(provider_type, provider, description, on_select)


def create_provider_card(provider_type: str, provider: str, description: str,
                         on_select: Callable[[str, str], None]) -> None:
    """
    Create a single provider card UI.

    Args:
        provider_type (str): The type/category of the provider (e.g., "oauth").
        provider (str): The provider's name.
        description (str): A short text describing the provider.
        on_select (Callable[[str, str], None]): Callback executed when the
            provider's "Select" button is clicked. Receives the provider type
            and provider name as arguments.

    Returns:
        None: This function creates UI elements but does not return a value.
    """
    with ui.card().classes(
        'w-64 h-56 cursor-pointer hover:bg-gray-50 transition-colors flex flex-col justify-between p-4'):
        with ui.column().classes('gap-y-1'):
            ui.icon('account_circle').classes('text-3xl text-primary')
            ui.label(provider.title()).classes('text-lg font-semibold')
            ui.label(description).classes('text-sm text-gray-600')

        ui.button('Select', on_click=lambda: on_select(provider_type, provider)).props(
            f'data-testid=select-{provider_type}-{provider}').classes('self-start')
