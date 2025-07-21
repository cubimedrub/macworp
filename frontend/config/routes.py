from nicegui import ui

from ..components.common.header import render_header
from ..pages import login, workflows, docs, dashboard
from ..pages.login import LoginPage
from ..pages.projects import projects_index, projects_new


def setup_routes():
    @ui.page('/')
    async def login_page():
        render_header()
        login = LoginPage()
        await login.show()

    @ui.page('/dashboard')
    def dashboard_page():
        render_header()
        return dashboard.show()

    @ui.page('/projects')
    async def projects_page():
        render_header()
        projects = projects_index.ProjectPage()
        await projects.show()

    @ui.page('/projects/new')
    def projects_page_new():
        render_header()
        new_projects = projects_new.ProjectPageNew()
        return new_projects.show()

    @ui.page('/workflows')
    def workflows_page():
        render_header()
        return workflows.show()

    @ui.page('/docs')
    def docs_page():
        render_header()
        return docs.show()
