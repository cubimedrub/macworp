from nicegui import ui, app, context

from macworp.configuration import Configuration

from macworp.configuration import Configuration

from ..components.common.cookie import create_cookie_banner
from ..components.common.header import render_header
from ..components.common.navigation import navigation_dashboard
from ..pages.login import LoginPage
from ..pages.projects import projects_index, projects_edit
from ..pages.workflows import workflow_index


def render_common_components(location: str):
    render_header(subtitle=location)
    if not app.storage.user.get("accepted_cookie"):
        create_cookie_banner()
        app.storage.user["accepted_cookie"] = True

    navigation_dashboard()


def setup_routes(config: Configuration):
    @ui.page("/")
    async def login_page():
        render_common_components("login")
        login = LoginPage(config)
        await login.show()

    @ui.page("/projects")
    async def projects_page():
        render_common_components("Projekts Index")
        if not app.storage.user.get("authenticated", False):
            ui.navigate.to("/")
            return
        projects = projects_index.ProjectsIndex(config, app.storage.user["auth-token"])
        await projects.show()

    @ui.page("/projects/edit")
    async def projects_page_edit():
        render_common_components("Projekt Details")
        project_id = None
        try:
            if hasattr(context, "client") and hasattr(context.client, "request"):
                request = context.client.request
                project_id = request.query_params.get("id")
        except:
            pass
        if not project_id:
            ui.label("project required").classes("test-danger")
            return None
        edit_projects = projects_edit.ProjectPageEdit(
            project_id, config, app.storage.user["auth-token"]
        )
        return await edit_projects.show()

    @ui.page("/workflows")
    async def workflows_page():
        render_common_components("Workflows")
        workflows = workflow_index.WorkflowIndex(config, app.storage.user["auth-token"])
        return await workflows.show()
