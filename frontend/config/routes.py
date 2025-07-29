from nicegui import ui, app

from ..components.common.header import render_header
from ..pages import login, workflows, docs, dashboard
from ..pages.login import LoginPage
from ..pages.projects import projects_index, projects_new, projects_edit
from nicegui import context


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
        if not app.storage.user.get('authenticated', False):
            ui.navigate.to('/')
            return
        render_header()
        projects = projects_index.ProjectsIndex()
        await projects.show()

    @ui.page('/projects/new')
    def projects_page_new():
        render_header()
        new_projects = projects_new.ProjectPageNew()
        return new_projects.show()

    @ui.page('/projects/edit')
    async def projects_page_edit():
        render_header()
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
def workflows_page():
    render_header()
    return workflows.show()


@ui.page('/docs')
def docs_page():
    render_header()
    return docs.show()
