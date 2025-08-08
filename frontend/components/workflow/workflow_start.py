from nicegui import ui

from frontend.services.workflow_service import WorkflowService


class WorkflowStart:
    def __init__(self, project):
        self.project = project
        self.workflows_service = WorkflowService()
        self.workflows = []

    async def get_workflows(self):
        self.workflows = await self.workflows_service.load_workflows()
        return self.workflows

    async def workflow_selection(self):
        await self.get_workflows()

        with ui.dialog().props('backdrop-filter="blur(8px) brightness(40%)"') as dialog, ui.card().classes('w-full'):
            self.dialog_content = ui.column()  # Container für dynamischen Inhalt

            with self.dialog_content:
                with ui.dropdown_button('Workflows', auto_close=True):
                    for workflow in self.workflows:
                        ui.item(workflow["name"] or workflow["id"],
                                on_click=lambda w=workflow: self.show_workflow_params(w))

            ui.button("New Workflow")

        await dialog

    async def show_workflow_params(self, selected_workflow):
        self.dialog_content.clear()
        workflow = await self.workflows_service.get_workflow(selected_workflow)
        params = workflow.get('definition', {}).get('parameters', {}).get('dynamic', [])
        with self.dialog_content:
            ui.label(f"Workflow: {selected_workflow['name']}").classes('text-h5')
            for param in params:
                param_type = param["type"]

                if param_type == "file-glob":
                    ui.input(param["label"], placeholder=param["desc"])

                elif param_type == "number":
                    ui.number(param["label"], placeholder=param["desc"])

                elif param_type == "text":
                    if param.get("is_multiline"):
                        ui.textarea(param["label"], placeholder=param["desc"])
                    else:
                        ui.input(param["label"], placeholder=param["desc"])

                elif param_type == "separator":
                    ui.separator()
                    if param.get("label"):
                        ui.label(param["label"])

                elif param_type == "value-select":
                    options = {opt["value"]: opt["label"] for opt in param["options"]}  # Umgedreht!
                    ui.select(
                        options=options,
                        label=param["label"],
                        value=param.get("value")
                    ).tooltip(param["desc"])

                elif param_type == "path":
                    multiple = param["desc"] in ["Multiple files", "Multiple folders"]
                    ui.upload(multiple=multiple).props(f'label="{param["label"]}"')

            ui.button("Delete Workflow", on_click=lambda:self.workflows_service.delete_workflow(selected_workflow))

def show_logs(self):
    logs = self.project.get('logs', [])
    if logs:
        ui.label("Logs").classes("text-h6")
        ui.textarea(value="\n".join(logs)).classes("w-full").props("rows=10")
