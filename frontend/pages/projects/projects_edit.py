from nicegui import ui

from frontend.components.project.file_viewer import FileViewer
from frontend.components.project.project_editor_table import ProjectEditTable
from frontend.services.project_service import ProjectService


class ProjectPageEdit:
    def __init__(self, project_id):
        self.project_service = ProjectService()
        self.project_id = project_id
        self.project = None
        self.file_viewer = FileViewer(project_id)
        self.project_path = None
        self.project_edit_table = ProjectEditTable(self.project, self.project_id)

    async def load_project_data(self):
        self.project = await self.project_service.load_project(self.project_id)

    async def load_project_path(self):
        self.project_path = await self.project_service.get_file_path(self.project_id)

    def start_workflow(self):
        self.show_progress()
        pass

    async def delete_project(self):
        await self.project_service.full_delete_project(self.project_id)
        ui.navigate.back()

    def create_project_table(self):
        """Create a dynamic table from project data"""
        if not self.project:
            ui.label("No project data available")
            return

        # Convert project dict to table rows
        rows = []
        for key, value in self.project.items():
            if isinstance(value, dict):
                display_value = str(value)
            elif isinstance(value, list):
                display_value = f"[{len(value)} items]" if value else "[]"
            elif value is None:
                display_value = "N/A"
            else:
                display_value = str(value)

            rows.append({
                'field': key.replace('_', ' ').title(),  # Format field name
                'value': display_value
            })
            columns = [
                {'name': 'field', 'label': 'Field', 'field': 'field', 'align': 'left'},
                {'name': 'value', 'label': 'Value', 'field': 'value', 'align': 'left'},
            ]

        # Create the table
        with ui.card().classes('w-full'):
            ui.label('Project Details').classes('text-h6 mb-4')
            ui.table(
                columns=columns,
                rows=rows,
                row_key='field'
            ).classes('w-full')

    async def editable_table(self):
        if not self.project:
            ui.notify('Project data not loaded', color='negative')
            return
        await self.project_edit_table.create_editable_table()

    def show_progress(self):
        if self.project.get('is_scheduled'):
            completed = self.project.get('completed_processes', 0)
            submitted = self.project.get('submitted_processes', 1)
            progress = completed / submitted if submitted else 0

            with ui.card().classes('w-full'):
                ui.label("Progress").classes("text-h6")
                ui.linear_progress(value=progress).props("color=green")
                ui.label("Hint: Progress may decrease during the run...").classes("text-caption text-grey")

    def show_logs(self):
        logs = self.project.get('logs', [])
        if logs:
            ui.label("Logs").classes("text-h6")
            ui.textarea(value="\n".join(logs)).classes("w-full").props("rows=10")
            ui.label("Hint: Logs are not persisted yet...").classes("text-caption text-grey")

    def show_error_report(self):
        error_report = self.project.get("error_report")
        if error_report:
            ui.label("Error report").classes("text-h6")
            ui.markdown(f"```\n{error_report}\n```").classes("text-red")
            ui.label("Hint: The error report is not persisted yet...").classes("text-caption text-grey")


    async def show(self):
        await self.load_project_data()
        await self.load_project_path()
        self.show_logs()
        self.show_error_report()
        if not self.project:
            ui.label('Project not found').classes('text-h6 text-center text-red')
            return

        with ui.row().classes("w-full justify-between items-center"):
            ui.label(f' {self.project["name"]}').classes('text-h4')
            ui.button("delete", on_click=self.delete_project, color='red')
            ui.button("Edit", on_click=self.editable_table)
            ui.button("Start Workflow", on_click=self.start_workflow(), color='green')
            files = await self.project_service.get_file_path(self.project_id)
            self.file_viewer.show_files(files)
            # todo ignore als eigenschaft fehlt
            # if self.project['ignore']:
            #     ui.badge('Currently ignored', color='warning')
