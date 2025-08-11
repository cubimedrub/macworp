from nicegui import ui


def render_header(title: str = "MACWorP", subtitle: str = ""):
    with ui.header().classes("bg-primary text-white"):
        with ui.column().classes("items-start justify-start w-full px-4 py-2 cursor-pointer hover:bg-primary-600").on('click', lambda: ui.navigate.to('/dashboard')):
            ui.label(title).classes("text-xl font-bold")
            if subtitle:
                ui.label(subtitle).classes("text-sm text-gray-200")
