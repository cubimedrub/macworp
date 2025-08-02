from nicegui import ui
import asyncio

from frontend.components.project.project_editor_table import ProjectEditTable
from frontend.services.project_service import ProjectService


class ProjectsIndex:
    def __init__(self):
        self.project_service = ProjectService()
        self.current_projects_page = 1
        self.projects_list = None
        self.pagination = None
        self.new_project_dialog = ProjectEditTable(None,None)
        self.show()
        asyncio.create_task(self.load_data())

    async def load_data(self):
        total_count = await self.project_service.load_count()
        self.pagination.max = total_count // ProjectService.PROJECTS_PER_PAGE + 1
        await self.load_projects()

    async def load_projects(self):
        projects = await self.project_service.load_projects(self.current_projects_page)
        self.update_projects_list(projects)

    def update_projects_list(self, projects):
        if projects is None or len(projects) == 0:
            self.projects_list.clear()
            ui.label("No projects found").classes("text-center mt-3")
            return

        self.projects_list.clear()
        for project in projects:
            with self.projects_list:
                project_name = project.get('name') if isinstance(project, dict) else getattr(project, 'name',
                                                                                             str(project))

                with self.projects_list:
                    with ui.card().classes("w-full mb-2"):
                        ui.link(project_name, f"/projects/edit?id={project['id']}").classes(
                            "text-decoration-none fw-bold text-lg")

                        if isinstance(project, dict):
                            if 'description' in project:
                                ui.label(project['description']).classes("text-muted small")

    def go_to_page(self, e):
        self.current_projects_page = e.value
        asyncio.create_task(self.load_projects())

    async def new_project(self):
        await self.new_project_dialog.create_editable_table()
        ui.navigate.reload()

    async def show(self):
        with ui.row().classes("w-full justify-between items-center"):
            ui.label("Projects").classes("text-2xl")
            ui.button("Start new project",on_click=self.new_project).classes("btn btn-primary btn-sm")

        self.projects_list = ui.column().classes("w-full")
        self.pagination = ui.pagination(
            value=self.current_projects_page,
            min=1,
            max=1,
            on_change=self.go_to_page
        ).classes("mt-4")
