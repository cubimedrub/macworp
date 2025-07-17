from nicegui import ui

from ..components.common.header import render_header
from ..pages import login, projects, workflows, docs, dashboard


def setup_routes():
    @ui.page('/')
    def login_page():
        render_header()
        return login.show()

    @ui.page('/dashboard')
    def dashboard_page():
        render_header()
        return dashboard.show()

    @ui.page('/projects')
    def projects_page():
        render_header()
        return projects.show()

    @ui.page('/workflows')
    def workflows_page():
        render_header()
        return workflows.show()

    @ui.page('/docs')
    def docs_page():
        render_header()
        return docs.show()
