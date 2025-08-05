from nicegui import ui


def navigation_dashboard():
    with ui.row().classes("l-full") :
        with ui.card().classes('fixed right-0 m-0 z-50'):
            ui.label("Dev Dashboard")
            ui.link("project","/projects")
            ui.link("login","/")
            ui.link("logout","/logout")
            ui.link("doku","/doku")
            ui.link("workflows","/workflows")