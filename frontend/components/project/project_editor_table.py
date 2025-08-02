from nicegui import ui

from frontend.services.project_service import ProjectService


class ProjectEditTable:
    """component for creating and or editing projects """

    def __init__(self, project, project_id):
        self.project = project
        self.project_service = ProjectService()
        self.project_id = project_id

    async def create_editable_table(self):
        """Create an editable table for project properties"""
        if not self.project:
            try:
                self.project = await self.project_service.load_project(self.project_id)
            except Exception as e:
                ui.notify(f'Error loading project: {str(e)}', color='negative')
                return

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

    def update_project_data(self, project):
        """Update the project data from parent"""
        self.project = project

    async def save_project(self, dialog):
        """Save the edited project data"""
        try:
            await self.project_service.update_project(self.project, self.project_id)
            dialog.close()
            ui.notify('Project saved successfully!', color='positive')
        except Exception as e:
            ui.notify(f'Error saving project: {str(e)}', color='negative')
