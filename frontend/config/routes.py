from nicegui import ui
from ..pages import home, login, projects, workflows, docs

def setup_routes():

    @ui.page('/')
    def index():
        return home.show()

    @ui.page('/login')
    def login_page():
        return login.show()

    @ui.page('/projects')
    def projects_page():
        return projects.show()

    @ui.page('/workflows')
    def workflows_page():
        return workflows.show()

    @ui.page('/docs')
    def docs_page():
        return docs.show()