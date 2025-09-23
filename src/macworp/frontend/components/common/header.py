from nicegui import ui


def render_header(title: str = "MACWorP", subtitle: str = "") -> None:
    """
    Renders generic Ui Header

    Args:
        title (str, optional): Large Page Title, Default "MACWorP"
        subtitle(str, optional): Current Page title.
    """
    with ui.header().classes("bg-primary text-white"):
        with ui.row().classes("w-full justify-between items-center "):
            with ui.column().classes('item-start'):
                ui.link(title, '/projects').classes("text-4xl font-bold text-white")
                ui.label(subtitle).classes("text-sm")
            ui.image('/images/logo.svg').classes('w-16 h-16 ml-auto')
            ui.button("logout",color = "red").classes("text-white")
            if subtitle:
                with ui.column().classes("items-start justify-start w-full px-4 py-2 "):
                    if subtitle != "login":
                        with ui.row().classes("space-x-4"):
                            ui.link("workflow", "\workflows").classes("ml-auto text-white")
                            ui.link("project", "\projects").classes("ml-auto text-white")

