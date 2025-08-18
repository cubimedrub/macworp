import json
from typing import Any

from nicegui import ui

from frontend.services.workflow_service import WorkflowService


class WorkflowEditorTable:
    def __init__(self, workflow: any, workflow_id: int | None = None):
        self.workflow = workflow
        self.workflow_service = WorkflowService()
        self.workflow_id = workflow_id

    async def create_editable_table(self):

        is_new_workflow = self.workflow_id is None and self.workflow is None

        if not is_new_workflow:
            try:
                self.workflow = await self.workflow_service.load_workflows(self.workflow_id)
            except Exception as e:
                ui.notify(f'Error loading workflow: {str(e)}', color='negative')
                return

        if is_new_workflow:
            self.workflow = self._get_workflow_template()

        dialog_title = 'Create New Workflow' if is_new_workflow else 'Edit Workflow'
        with (ui.dialog().props('backdrop-filter="blur(8px) brightness(40%)"') as dialog, ui.card().classes('w-full')):
            ui.label(dialog_title).classes('text-h6 mb-4 center')

            with ui.column(wrap=True).classes('w-full gap-4 border p-1'):
                for key, value in self.workflow.items():
                    if key in ['id', 'created_at', 'updated_at', 'workflow_id', 'workflow_arguments', 'is_published']:
                        continue

                    ui.label(key.replace('_', ' ').title()).classes('text-left font-medium')

                    if key == 'definition':
                        ui.textarea(
                                    placeholder='Workflow definition').bind_value_to(self.workflow, key)

                    elif isinstance(value, list):
                        list_str = ', '.join(str(item) for item in value) if value else ''
                        list_input = ui.input(value=list_str, placeholder='Comma-separated values')
                        list_input.on('blur', lambda e, k=key: self._update_list_field(k, e.sender.value))
                    else:
                        ui.input(value=str(value), placeholder='').bind_value_to(self.workflow, key)

                save_button_text = 'Create Workflow' if is_new_workflow else 'Save Changes'
                ui.button(save_button_text, color='primary',
                          on_click=lambda: self._save_workflow(dialog, is_new_workflow))

            await dialog


    def _get_workflow_template(self) -> dict[str, str | None | dict[Any, Any] | bool]:
        """
       Return a default template for a new workflow.

       Returns:
           dict: A dictionary with default workflow fields.
       """
        return {
            'name': '',
            'description': '',
            'definition': ''
        }

    def _update_list_field(self, key: str, value_str: str) -> None:
        """
        Update a list-type field from a comma-separated string.

        Args:
            key (str): The workflow field to update.
            value_str (str): Comma-separated string representing list items.

        Returns:
            None
        """
        if value_str.strip():
            self.workflow[key] = [item.strip() for item in value_str.split(',') if item.strip()]
        else:
            self.workflow[key] = []

    async def _save_workflow(self, dialog, is_new_workflow) -> None:

        try:
            await self.workflow_service.save_workflow(is_new_workflow)
            dialog.close()
        except Exception as e:
            ui.notify(f'Error saving workflow: {str(e)}', color='negative')
            return
