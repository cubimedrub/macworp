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

        # Check if this is for creating a new project
        is_new_project = self.project_id is None or self.project is None

        # Load project data if editing existing project
        if not is_new_project and not self.project:
            try:
                self.project = await self.project_service.load_project(self.project_id)
            except Exception as e:
                ui.notify(f'Error loading project: {str(e)}', color='negative')
                return

        # For new projects, create empty template
        if is_new_project:
            self.project = self.get_project_template()

        if not self.project:
            ui.notify('Could not load or create project data', color='negative')
            return

        dialog_title = 'Create New Project' if is_new_project else 'Edit Project'

        with (ui.dialog().props('backdrop-filter="blur(8px) brightness(40%)"') as dialog, ui.card().classes('w-full')):
            ui.label(dialog_title).classes('text-h6 mb-4 center')

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
                        list_input = ui.input(value=list_str, placeholder='Comma-separated values')
                        list_input.on('blur', lambda e, k=key: self.update_list_field(k, e.sender.value))
                    elif isinstance(value, dict):
                        dict_input = ui.textarea(value=str(value), placeholder='JSON format')
                        dict_input.on('blur', lambda e, k=key: self.update_dict_field(k, e.sender.value))
                    elif value is None:
                        ui.input(value='', placeholder='Empty').bind_value_to(self.project, key)
                    else:
                        ui.input(value=str(value)).bind_value_to(self.project, key)

            save_button_text = 'Create Project' if is_new_project else 'Save Changes'
            ui.button(save_button_text, color='primary', on_click=lambda: self.save_project(dialog, is_new_project))

        await dialog

    def get_project_template(self):
        """Get a template for new projects"""
        return {
            'name': '',
            'workflow_id': None,
            'workflow_arguments': {},
            'description': '',
            'is_published': False
        }

    def update_list_field(self, key, value_str):
        """Update list field from comma-separated string"""
        if value_str.strip():
            self.project[key] = [item.strip() for item in value_str.split(',') if item.strip()]
        else:
            self.project[key] = []

    def update_dict_field(self, key, value_str):
        """Update dict field from string representation"""
        try:
            import json
            if value_str.strip():
                self.project[key] = json.loads(value_str)
            else:
                self.project[key] = {}
        except:
            ui.notify(f'Invalid JSON format for {key}', color='warning')

    async def save_project(self, dialog, is_new_project):
        """Save the edited project data"""
        try:
            if is_new_project:
                # Clean up data before sending
                project_data = self.project.copy()

                # Convert workflow_id: empty string -> None, valid string -> int
                workflow_id = project_data.get('workflow_id')
                if workflow_id == '' or workflow_id is None:
                    project_data['workflow_id'] = None
                elif isinstance(workflow_id, str):
                    try:
                        project_data['workflow_id'] = int(workflow_id)
                    except ValueError:
                        project_data['workflow_id'] = None

                # Ensure workflow_arguments is a dict
                if isinstance(project_data.get('workflow_arguments'), str):
                    try:
                        import json
                        project_data['workflow_arguments'] = json.loads(project_data['workflow_arguments']) if \
                        project_data['workflow_arguments'].strip() else {}
                    except:
                        project_data['workflow_arguments'] = {}

                # Create new project
                new_project = await self.project_service.create_project(project_data)
                self.project_id = new_project if isinstance(new_project, int) else new_project.get('id')
                ui.notify('Project created successfully!', color='positive')
            else:
                # Update existing project
                await self.project_service.update_project(self.project, self.project_id)
                ui.notify('Project saved successfully!', color='positive')

            dialog.close()

        except Exception as e:
            ui.notify(f'Error saving project: {str(e)}', color='negative')
