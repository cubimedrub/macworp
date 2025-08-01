from nicegui import ui

from frontend.components.project.file_viewer import FileViewer
from frontend.services.project_service import ProjectService


class ProjectPageEdit:
    def __init__(self, project_id):
        self.project_service = ProjectService()
        self.project_id = project_id
        self.project = None
        self.file_viewer = FileViewer(project_id)
        self.project_path = None

    async def load_project_data(self):
        self.project = await self.project_service.load_project(self.project_id)

    async def load_project_path(self):
        self.project_path = await self.project_service.get_file_path(self.project_id)

    def start_workflow(self):
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

    async def create_editable_table(self):
        """Create an editable table for project properties"""

        # Create editable rows
        with (ui.dialog().props('backdrop-filter="blur(8px) brightness(40%)"') as dialog, ui.card().classes('w-full')):
            ui.label('Edit Project').classes('text-h6 mb-4 center')

            with ui.column(wrap=True).classes('w-full gap-4 border p-1'):
                for key, value in self.project.items():
                    if key in ['id', 'created_at', 'updated_at']:  # Skip non-editable fields
                        continue
                    ui.label(key.replace('_', ' ').title()).classes('text-left font-medium')
                    if isinstance(value, bool):
                        ui.checkbox(value=value).bind_value_to(self.project, key)
                    elif isinstance(value, int):
                        ui.number(value=value).bind_value_to(self.project, key)
                    elif isinstance(value, float):
                        ui.number(value=value, format='%.2f').bind_value_to(self.project, key)
                    elif isinstance(value, list):
                        list_str = ', '.join(str(item) for item in value) if value else ''
                        ui.input(value=list_str, placeholder='Comma-separated values')
                    elif isinstance(value, dict):
                        ui.textarea(value=str(value))
                    elif value is None:
                        ui.input(value='', placeholder='Empty').bind_value_to(self.project, key)
                    else:
                        ui.input(value=str(value)).bind_value_to(self.project, key)

            ui.button('Save Changes', color='primary', on_click=lambda: self.save_project(dialog))

        await dialog

    async def save_project(self, dialog):
        """Save the edited project data"""
        try:
            await self.project_service.update_project(self.project, self.project_id)
            dialog.close()
            ui.notify('Project saved successfully!', color='positive')
        except Exception as e:
            ui.notify(f'Error saving project: {str(e)}', color='negative')

    async def show(self):
        await self.load_project_data()
        await self.load_project_path()
        if not self.project:
            ui.label('Project not found').classes('text-h6 text-center text-red')
            return

        with ui.row().classes("w-full justify-between items-center"):
            ui.label(f' {self.project["name"]}').classes('text-h4')
            ui.button("delete", on_click=self.delete_project, color='red')
            ui.button("Edit", on_click=self.create_editable_table)
            files = await self.project_service.get_file_path(self.project_id)
            self.file_viewer.show_files(files)
            # todo ignore als eigenschaft fehlt
            # if self.project['ignore']:
            #     ui.badge('Currently ignored', color='warning')
