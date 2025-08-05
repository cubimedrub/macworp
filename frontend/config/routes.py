from nicegui import ui, app

from ..components.common.cookie import create_cookie_banner
from ..components.common.header import render_header
from ..components.common.navigation import navigation_dashboard
from ..pages import login, workflows, docs, dashboard
from ..pages.login import LoginPage
from ..pages.projects import projects_index, projects_edit
from ..pages.workflows import workflow_index
from nicegui import context

def render_common_components(location: str):
    render_header(subtitle=location)
    create_cookie_banner()
    navigation_dashboard()

def setup_routes():
    @ui.page('/')
    async def login_page():
        render_common_components("login")
        login = LoginPage()
        await login.show()

    @ui.page('/dashboard')
    def dashboard_page():
        render_common_components("dashboard")
        render_header()
        return dashboard.show()

    @ui.page('/projects')
    async def projects_page():
        render_common_components("Projekts Index")
        if not app.storage.user.get('authenticated', False):
            ui.navigate.to('/')
            return
        render_header()
        projects = projects_index.ProjectsIndex()
        await projects.show()

    # @ui.page('/projects/new')
    # def projects_page_new():
    #     render_header()
    #     new_projects = projects_new.ProjectPageNew()
    #     return new_projects.show()

    @ui.page('/projects/edit')
    async def projects_page_edit():
        render_common_components("Projekt Details")
        project_id = None
        try:
            if hasattr(context, 'client') and hasattr(context.client, 'request'):
                request = context.client.request
                project_id = request.query_params.get('id')
        except:
            pass
        if not project_id:
            ui.label("project required").classes("test-danger")
            return None
        edit_projects = projects_edit.ProjectPageEdit(project_id)
        await edit_projects.show()


    @ui.page('/workflows')
    async def workflows_page():
        render_common_components("Workflows")
        render_header()
        workflows = workflow_index.WorkflowIndex()
        return workflows.show()


@ui.page('/docs')
def docs_page():
    render_header()
    return docs.show()
