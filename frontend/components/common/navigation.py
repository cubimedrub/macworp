from nicegui import ui


def navigation_dashboard():
    with ui.row().classes("w-full justify-end p-4"):
        with ui.card().classes('bg-primary text-white'):
            ui.label("Dev Dashboard").classes("text-xl font-bold")
            ui.link("project", "/projects").classes("text-white")
            ui.link("login", "/").classes("text-white")
            ui.link("logout", "/logout").classes("text-white")
            ui.link("doku", "/doku").classes("text-white")
            ui.link("workflows", "/workflows").classes("text-white")
