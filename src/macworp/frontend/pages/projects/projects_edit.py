from nicegui import ui

from macworp.configuration import Configuration
from macworp.frontend.components.project.file_viewer import FileViewer
from macworp.frontend.components.project.project_editor_table import ProjectEditTable
from macworp.frontend.components.workflow.workflow_start import WorkflowStart
from macworp.frontend.services.project_service import ProjectService
from typing import Dict, List, Any, Optional


class ProjectPageEdit:
    """
    Handles the project edit page UI and interactions.

    This class manages the display and editing of project details, including
    file viewing, workflow execution, and project management operations.

    Attributes:
        project_service (ProjectService): Service for project operations
        project_id (int): ID of the current project
        project (Optional[Dict[str, Any]]): Current project data
        file_viewer (FileViewer): Component for viewing project files
        project_path (Optional[str]): Path to project files
        project_edit_table (ProjectEditTable): Component for editing project data
        workflow_start (WorkflowStart): Component for starting workflows
    """

    def __init__(self, project_id: int, config: Configuration, auth_token: str):
        """
        Initialize the project edit page.

        Args:
            project_id (int): ID of the project to edit
        """
        self.project_service = ProjectService(config, auth_token)
        self.project_id = project_id
        self.project: Optional[Dict[str, Any]] = None
        self.file_viewer = FileViewer(config, project_id)
        self.project_path: Optional[str] = None
        self.project_edit_table = ProjectEditTable(
            self.project, self.project_id, config, auth_token
        )
        self.workflow_start = WorkflowStart(self.project_id, config, auth_token)

    async def load_project_data(self) -> None:
        """
        Load project data from the service.

        Returns:
            None
        """
        self.project = await self.project_service.load_project(self.project_id)

    async def load_project_path(self) -> None:
        """
        Load the file path for the project.

        Returns:
            None
        """
        self.project_path = await self.project_service.get_file_path(self.project_id)

    async def start_workflow(self) -> None:
        """
        Start workflow selection dialog for the current project.

        Returns:
            None
        """
        self.workflow_start.project_id = self.project_id
        await self.workflow_start.workflow_selection()

    async def delete_project(self) -> None:
        """
        Delete the current project and navigate back.

        Returns:
            None
        """
        await self.project_service.full_delete_project(self.project_id)
        ui.navigate.back()

    async def editable_table(self) -> None:
        """
        Display an editable table for project data.

        Shows a notification if project data is not loaded, otherwise
        creates an editable table interface.

        Returns:
            None
        """
        if not self.project:
            ui.notify("Project data not loaded", color="negative")
            return
        await self.project_edit_table.create_editable_table()
        ui.navigate.reload()

    async def change_owner(self) -> None:
        """
        Display dialog for changing project ownership.

        Shows a dialog allowing the user to transfer ownership of the project
        to another user by entering their ID.

        Returns:
            None
        """
        with (
            ui.dialog().props('backdrop-filter="blur(8px) brightness(40%)"') as dialog,
            ui.card().classes("w-full"),
        ):
            ui.label("Transfer Ownership of Project")

            with ui.card().classes("w-full"):
                new_owner = ui.input("New Owner ID").classes("w-full").props("outlined")

            async def confirm_change() -> None:
                """
                Confirm and execute ownership change.

                Returns:
                    None
                """
                response = await self.project_service.change_user(
                    new_owner.value, self.project, self.project_id
                )
                if response:
                    ui.notify(f"Owner changed to {new_owner.value}")
                    dialog.close()
                else:
                    ui.notify("Owner not found")

            ui.button("Confirm", on_click=confirm_change).props("color=primary")
            ui.button("Cancel", on_click=dialog.close)

        await dialog

    async def show(self) -> None:
        """
        Display the complete project edit page.

        Loads project data and displays all project information including
        details, files, progress, and available actions.

        Returns:
            None
        """
        await self.load_project_data()
        await self.load_project_path()
        self._show_error_report()

        if not self.project:
            ui.label("Project not found").classes("text-h6 text-center text-red")
            return

        self._render_header()
        files = await self.project_service.get_file_path(self.project_id)
        self.file_viewer.show_files(files)

    def _render_header(self) -> None:
        """
        Render the project header with title and action buttons.

        Returns:
            None
        """
        with ui.row().classes("w-full justify-between items-center"):
            ui.label(f' {self.project["name"]}').classes("text-h4")

            with ui.button_group():
                ui.button("delete", on_click=self.delete_project, color="red")
                ui.button("Edit", on_click=self.editable_table)
                ui.button("Start Workflow", on_click=self.start_workflow, color="green")
                ui.button("Change Owner", on_click=lambda: self.change_owner())

    def _create_project_table(self) -> None:
        """
        Create a dynamic table displaying project data.

        Converts project dictionary data into a readable table format,
        handling different data types appropriately.

        Returns:
            None
        """
        if not self.project:
            ui.label("No project data available")
            return

        rows = self._convert_project_to_table_rows()
        columns = self._get_table_columns()

        with ui.card().classes("w-full"):
            ui.label("Project Details").classes("text-h6 mb-4")
            ui.table(columns=columns, rows=rows, row_key="field").classes("w-full")

    def _convert_project_to_table_rows(self) -> List[Dict[str, str]]:
        """
        Convert project data to table row format.

        Returns:
            List[Dict[str, str]]: List of table rows with field and value keys
        """
        rows = []
        for key, value in self.project.items():
            display_value = self._format_display_value(value)
            rows.append(
                {"field": key.replace("_", " ").title(), "value": display_value}
            )
        return rows

    def _format_display_value(self, value: Any) -> str:
        """
        Format a value for display in the table.

        Args:
            value (Any): The value to format

        Returns:
            str: Formatted display string
        """
        if isinstance(value, dict):
            return str(value)
        elif isinstance(value, list):
            return f"[{len(value)} items]" if value else "[]"
        elif value is None:
            return "N/A"
        else:
            return str(value)

    def _get_table_columns(self) -> List[Dict[str, str]]:
        """
        Get table column definitions.

        Returns:
            List[Dict[str, str]]: List of column definitions
        """
        return [
            {"name": "field", "label": "Field", "field": "field", "align": "left"},
            {"name": "value", "label": "Value", "field": "value", "align": "left"},
        ]

    def _show_progress(self) -> None:
        """
        Display project progress if the project is scheduled.

        Shows a progress bar with completion status and helpful hints.

        Returns:
            None
        """
        if not self.project.get("is_scheduled"):
            return

        completed = self.project.get("completed_processes", 0)
        submitted = self.project.get("submitted_processes", 1)
        progress = completed / submitted if submitted else 0

        with ui.card().classes("w-full"):
            ui.label("Progress").classes("text-h6")
            ui.linear_progress(value=progress).props("color=green")
            ui.label("Hint: Progress may decrease during the run...").classes(
                "text-caption text-grey"
            )

    def _show_error_report(self) -> None:
        """
        Display an error report if available.

        Shows any error report associated with the project in a formatted
        code block with appropriate styling.

        Returns:
            None
        """
        if not self.project:
            return

        error_report = self.project.get("error_report")
        if error_report:
            ui.label("Error report").classes("text-h6")
            ui.markdown(f"```\n{error_report}\n```").classes("text-red")
            ui.label("Hint: The error report is not persisted yet...").classes(
                "text-caption text-grey"
            )
