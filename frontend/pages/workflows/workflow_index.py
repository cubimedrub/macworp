from nicegui import ui

from frontend.services.workflow_service import WorkflowService


class WorkflowIndex:
    def __init__(self):
        self.workflow_service = WorkflowService()
        self.workflow_list = None

    async def load_workflows(self):
        workflows = await self.workflow_service.load_workflows()
        self.workflow_list(workflows)
    async def delete_workflow(self):
        pass

    def workflow_table(self, workflows):
        if workflows is None or len(workflows) == 0:
            self.workflow_list.clear()
            ui.label("No workflows found").classes("text-center mt-3")
            return
        self.workflow_list.clear()

        for workflow in workflows:
            with self.workflow_list:
                workflow_name = workflow.get('name') if isinstance(workflow, dict) else getattr(workflow, 'name', str(workflow))
                with self.workflow_list:
                    with ui.card().classes("w-full mb-2"):
                        ui.label(workflow_name).classes("text-decoration-none fw-bold text-lg")
                        if isinstance(workflow, dict):
                            if 'description' in workflow:
                                ui.label(workflow['description']).classes("text-muted small")
                            if  workflow['is_published']:
                                ui.icon("Lock Open")
                            else:
                                ui.icon("Lock")
                        ui.button("Delete")

    async def show(self):
        with ui.row().classes("w-full justify-between items-center"):
            ui.label("Workflows").classes("test-2xl")
