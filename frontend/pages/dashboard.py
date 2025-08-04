#from frontend.components.common.navigation import render_navigation
from nicegui import ui


def show():
    "Dashboard Page"
    # if not AuthService.is_authenticated():
    #     ui.navigate.to('/login')
    ui.navigate.to('/projects')
   # render_navigation()
