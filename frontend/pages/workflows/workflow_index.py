from nicegui import ui

from frontend.services.workflow_service import WorkflowService


class WorkflowIndex:
    def __init__(self):
        self.workflow_service = WorkflowService()
        self.workflow_list = None

    async def load_workflows(self):
        """Load workflows from service and return them"""
        workflows = await self.workflow_service.load_workflows()
        return workflows

    async def delete_workflow(self, workflow_id):
        # todo delete
        pass

    async def workflow_table(self):
        if self.workflow_list is None:
            self.workflow_list = ui.column().classes("w-full")

        workflows = await self.load_workflows()

        if workflows is None or len(workflows) == 0:
            self.workflow_list.clear()
            with self.workflow_list:
                ui.label("No workflows found").classes("text-center mt-3")
            return

        self.workflow_list.clear()

        for workflow in workflows:
            with self.workflow_list:
                workflow_name = workflow.get('name') if isinstance(workflow, dict) else getattr(workflow, 'name',
                                                                                                str(workflow))

                with ui.card().classes("w-full mb-2"):
                    ui.label(workflow_name).classes("text-decoration-none fw-bold text-lg")

                    if isinstance(workflow, dict):
                        if 'description' in workflow:
                            ui.label(workflow['description']).classes("text-muted small")

                        # Check if workflow is published
                        if workflow.get('is_published', False):
                            ui.icon("lock_open")
                        else:
                            ui.icon("lock")

                    ui.button("Delete", on_click=lambda w=workflow: self.delete_workflow(w))

    async def show(self):
        with ui.column().classes("w-full"):
            with ui.row().classes("w-full justify-between items-center mb-4"):
                ui.label("Workflows").classes("text-2xl")

            self.workflow_list = ui.column().classes("w-full")

            await self.workflow_table()
