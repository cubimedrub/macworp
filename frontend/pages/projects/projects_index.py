from typing import List, Dict, Any, Optional, Union
import asyncio
from nicegui import ui

from frontend.components.project.project_editor_table import ProjectEditTable
from frontend.services.project_service import ProjectService


class ProjectsIndex:
    """
    Handles the projects index page UI and interactions.

    This class manages the display of projects list with pagination,
    project creation, and project sharing functionality.

    Attributes:
        project_service (ProjectService): Service for project operations
        current_projects_page (int): Current page number for pagination
        projects_list (Optional[ui.column]): UI column containing the projects list
        pagination (Optional[ui.pagination]): UI pagination component
        new_project_dialog (ProjectEditTable): Component for creating new projects
    """

    def __init__(self):
        """
        Initialize the projects index page.

        Sets up the UI components and starts the data loading process.
        """
        self.project_service = ProjectService()
        self.current_projects_page: int = 1
        self.projects_list: Optional[ui.column] = None
        self.pagination: Optional[ui.pagination] = None
        self.new_project_dialog = ProjectEditTable(None, None)

        asyncio.create_task(self._load_data())

    async def _load_data(self) -> None:
        """
        Load initial data including project count and projects list.

        Sets up pagination based on total project count and loads the first page.

        Returns:
            None
        """
        total_count = await self.project_service.load_count()
        self.pagination.max = total_count // ProjectService.PROJECTS_PER_PAGE + 1
        await self._load_projects()

    async def _load_projects(self) -> None:
        """
        Load projects for the current page.

        Fetches projects from the service and updates the UI list.

        Returns:
            None
        """
        projects = await self.project_service.load_projects(self.current_projects_page)
        self._update_projects_list(projects)

    def _update_projects_list(self, projects: Optional[List[Dict[str, Any]]]) -> None:
        """
        Update the projects list UI with new project data.

        Args:
            projects (Optional[List[Dict[str, Any]]]): List of project dictionaries

        Returns:
            None
        """
        if not projects:
            self._show_empty_projects_message()
            return

        self.projects_list.clear()
        for project in projects:
            self._render_project_card(project)

    def _show_empty_projects_message(self) -> None:
        """
        Display message when no projects are found.

        Returns:
            None
        """
        self.projects_list.clear()
        with self.projects_list:
            ui.label("No projects found").classes("text-center mt-3")

    def _render_project_card(self, project: Dict[str, Any]) -> None:
        """
        Render a single project card in the list.

        Args:
            project (Dict[str, Any]): Project data dictionary

        Returns:
            None
        """
        project_name = self._get_project_name(project)

        with self.projects_list:
            with ui.card().classes("relative w-full mb-2"):
                ui.link(project_name, f"/projects/edit?id={project['id']}").classes("text-lg")
                ui.button(
                    "Share",
                    on_click=lambda p=project: self._share_project(p)
                ).classes("absolute top-4 right-1 size-12")

                if project.get('description'):
                    ui.label(project['description']).classes(
                        "text-muted small align-baseline"
                    )

    def _get_project_name(self, project: Union[Dict[str, Any], Any]) -> str:
        """
        Extract project name from project object.

        Args:
            project (Union[Dict[str, Any], Any]): Project data as dict or object

        Returns:
            str: Project name or string representation
        """
        if isinstance(project, dict):
            return project.get('name', str(project))
        return getattr(project, 'name', str(project))

    def _go_to_page(self, e: Any) -> None:
        """
        Handle pagination page change event.

        Args:
            e (Any): Event object containing the new page value

        Returns:
            None
        """
        self.current_projects_page = e.value
        asyncio.create_task(self._load_projects())

    async def _new_project(self) -> None:
        """
        Create a new project dialog and reload page after creation.

        Returns:
            None
        """
        await self.new_project_dialog.create_editable_table()
        ui.navigate.reload()

    async def _share_project(self, project: Dict[str, Any]) -> None:
        """
        Display project sharing dialog.

        Shows a dialog allowing the user to add other users to the project
        with specified access rights.

        Args:
            project (Dict[str, Any]): Project data to share

        Returns:
            None
        """
        with (ui.dialog().props('backdrop-filter="blur(8px) brightness(40%)"') as dialog,
              ui.card().classes('w-full')):
            ui.label("Add Authors to your Projects")

            with ui.card().classes('w-full'):
                added_owner = ui.input('Owner ID').classes('w-full').props('outlined')
                access_rights = ui.select(
                    label='Access rights',
                    options=['read', 'write'],
                    value='read'
                )

            async def confirm_change() -> None:
                """
                Confirm and execute project sharing.

                Returns:
                    None
                """
                write_access = access_rights.value == 'write'
                response = await self.project_service.add_share_project(
                    added_owner.value,
                    project,
                    project['id'],
                    write_access
                )

                if response:
                    ui.notify("User added")
                    dialog.close()
                else:
                    ui.notify("Owner not found")

            ui.button("Confirm", on_click=confirm_change).props("color=primary")
            ui.button("Cancel", on_click=dialog.close)

        await dialog

    async def show(self) -> None:
        """
        Display the main projects index UI.

        Sets up the page layout including the new project button,
        projects list container, and pagination controls.

        Returns:
            None
        """
        with ui.row().classes("w-full justify-between items-center"):
            ui.button(
                "Start new project",
                on_click=self._new_project
            ).classes("btn btn-primary btn-sm")

        self.projects_list = ui.column().classes("w-full")
        self.pagination = ui.pagination(
            value=self.current_projects_page,
            min=1,
            max=1,
            on_change=self._go_to_page
        ).classes("mt-4")
