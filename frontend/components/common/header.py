from nicegui import ui


def render_header(title: str = "MACWorP", subtitle: str = "")->None:
    """
    Renders generic Ui Header

    Args:
        title (str, optional): Large Page Title, Default "MACWorP"
        subtitle(str, optional): Current Page title.
    """
    with ui.header().classes("bg-primary text-white"):
        with ui.column().classes("items-start justify-start w-full px-4 py-2 cursor-pointer hover:bg-primary-600").on(
            'click', lambda: ui.navigate.to('/projects')):
            ui.label(title).classes("text-xl font-bold")
            if subtitle:
                ui.label(subtitle).classes("text-sm text-gray-200")
