from nicegui import ui


def render_header(title: str= "MACWorP" , subtitle: str = ""):
    with ui.header().classes("bg-primary text-white"):
        with ui.row().classes("items-center justify-between w-full px-4 py-2"):
            ui.label(title).classes("text-xl font-bold")
            if subtitle:
                ui.label(subtitle).classes("text-sm text-gray-200")
