from nicegui import ui
from services.auth_service import AuthService

from frontend.pages import home, login, projects, workflows, docs


def setup_routes():
    auth_service = AuthService()

    @ui.page('/')
    def index():
        return home.show()

    @ui.page('/login')
    def login_page():
        return login.show()

    @ui.page('/projects')
    def projects_page():
        if not auth_service.is_authenticated():
            ui.navigate.to('/login')
            return None
        return projects.show()

    @ui.page('/workflows')
    def workflows_page():
        if not auth_service.is_authenticated():
            ui.navigate.to('/login')
            return None
        return workflows.show()

    @ui.page('/docs')
    def docs_page():
        return docs.show()
