from frontend.components.common.navigation import render_navigation


def show():
    "Dashboard Page"
    # if not AuthService.is_authenticated():
    #     ui.navigate.to('/login')
    render_navigation()
