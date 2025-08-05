from nicegui import ui

"""
Dirks funny Cookie Banner
"""
def create_cookie_banner():
    with ui.card().classes('fixed bottom-0 left-0 m-4 z-50') as banner:
        with ui.row().classes('items-center'):
            ui.label('Dirk will deine Cookies du hast da keine wahl').classes('text-sm')
            ui.button('Verstanden', on_click=lambda: banner.delete()).classes('ml-2')

