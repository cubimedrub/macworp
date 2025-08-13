from nicegui import ui
from frontend.services.workflow_service import WorkflowService


class WorkflowStart:
    def __init__(self, project_id):
        self.project_id = project_id
        self.workflow = None
        self.workflows_service = WorkflowService()
        self.workflows = []
        self.uploaded_files = {}  # Stores uploaded files per parameter

    async def get_workflows(self):
        self.workflows = await self.workflows_service.load_workflows()
        return self.workflows

    async def workflow_selection(self):
        await self.get_workflows()

        with ui.dialog().props('backdrop-filter="blur(8px) brightness(40%)"') as dialog, ui.card().classes('w-full'):
            self.dialog_content = ui.column()

            with self.dialog_content:
                with ui.dropdown_button('Workflows', auto_close=True):
                    for workflow in self.workflows:
                        ui.item(workflow["name"] or workflow["id"],
                                on_click=lambda w=workflow: self.show_workflow_params(w))

            ui.button("New Workflow")

        await dialog

    async def show_workflow_params(self, selected_workflow):
        self.workflow = selected_workflow
        self.dialog_content.clear()
        workflow = await self.workflows_service.get_workflow(selected_workflow)
        params = workflow.get('definition', {}).get('parameters', {}).get('dynamic', [])

        # Stores UI elements and parameter definitions
        self.parameter_inputs = {}
        self.param_mapping = {}

        with self.dialog_content:
            ui.label(f"Workflow: {selected_workflow['name']}").classes('text-h5')

            for param in params:
                param_type = param["type"]
                param_name = param["label"]
                param_id = param.get("id", param_name)
                self.param_mapping[param_id] = param

                if param_type == "file-glob":
                    input_element = ui.input(param["label"], placeholder=param["desc"])
                    self.parameter_inputs[param_id] = input_element

                elif param_type == "number":
                    input_element = ui.number(param["label"], placeholder=param["desc"])
                    self.parameter_inputs[param_id] = input_element

                elif param_type == "text":
                    if param.get("is_multiline"):
                        input_element = ui.textarea(param["label"], placeholder=param["desc"])
                    else:
                        input_element = ui.input(param["label"], placeholder=param["desc"])
                    self.parameter_inputs[param_id] = input_element

                elif param_type == "separator":
                    ui.separator()
                    if param.get("label"):
                        ui.label(param["label"])

                elif param_type == "value-select":
                    options = {opt["value"]: opt["label"] for opt in param["options"]}
                    input_element = ui.select(
                        options=options,
                        label=param["label"],
                        value=param.get("value")
                    ).tooltip(param["desc"])
                    self.parameter_inputs[param_id] = input_element

                elif param_type in ("path", "paths"):  # angepasst für single und multiple paths
                    desc = param.get("desc", "")
                    multiple = param_type == "paths" or ("folder" in desc.lower()) or ("Multiple" in desc)

                    input_element = ui.upload(
                        multiple=multiple,
                        auto_upload=True,
                        label=param["label"],
                        on_upload=lambda e, pid=param_id: self._store_uploaded_files(pid, e)
                    )

                    upload_id = f"upload_{param_id.replace(' ', '_')}"
                    input_element._props['id'] = upload_id

                    if "folder" in desc.lower():
                        await ui.run_javascript(f'''
                            const uploadElement = document.getElementById('{upload_id}');
                            if (uploadElement) {{
                                const input = uploadElement.querySelector('input[type="file"]');
                                if (input) {{
                                    input.setAttribute('webkitdirectory', '');
                                    input.setAttribute('directory', '');
                                    input.setAttribute('multiple', '');
                                }}
                            }}
                        ''')
                    elif multiple:
                        await ui.run_javascript(f'''
                            const uploadElement = document.getElementById('{upload_id}');
                            if (uploadElement) {{
                                const input = uploadElement.querySelector('input[type="file"]');
                                if (input && !input.hasAttribute('multiple')) {{
                                    input.setAttribute('multiple', '');
                                }}
                            }}
                        ''')

                    self.parameter_inputs[param_id] = input_element

            ui.button("Delete Workflow",
                      on_click=lambda w=selected_workflow: self.workflows_service.delete_workflow(w))
            ui.button("Start Workflow",
                      on_click=self.start_workflow_with_params)

    def _store_uploaded_files(self, param_id, event):
        """Stores uploaded file names per parameter"""
        if param_id not in self.uploaded_files:
            self.uploaded_files[param_id] = []
        self.uploaded_files[param_id].append(event.name)
        print(f"Uploaded for {param_id}: {event.name}")

    async def start_workflow_with_params(self):
        user_parameter_values = {}

        for param_id, input_element in self.parameter_inputs.items():
            param_def = self.param_mapping.get(param_id, {})
            param_type = param_def.get("type")
            value = None

            if param_type in ("path", "paths"):
                files = self.uploaded_files.get(param_id)
                if files:
                    value = files if len(files) > 1 else files[0]
            else:
                if hasattr(input_element, 'value'):
                    value = input_element.value

            user_parameter_values[param_id] = value
            print(f"Parameter '{param_id}' (type: {param_type}): {value}")

        # Umwandeln in Liste von Dicts
        workflow_parameters = []
        for param_id, value in user_parameter_values.items():
            param_def = self.param_mapping.get(param_id, {})
            workflow_parameters.append({
                "name": param_def.get("name", param_id),  # oder "label", je nachdem was Backend erwartet
                "label": param_def.get("label", param_id),
                "type": param_def.get("type"),
                "value": value
            })

        # Check for missing required parameters (optional)
        missing_params = []
        for param_id, param_def in self.param_mapping.items():
            param_label = param_def.get("label", param_id)
            if param_label in [p["label"] for p in workflow_parameters]:
                value = next(p["value"] for p in workflow_parameters if p["label"] == param_label)
                if value is None or value == "" or (isinstance(value, list) and len(value) == 0):
                    missing_params.append(param_label)

        if missing_params:
            ui.notify(f"Missing required parameters: {', '.join(missing_params)}", type='negative')
            return

        try:
            await self.workflows_service.start_workflow(self.project_id, self.workflow, workflow_parameters)
            ui.notify("Workflow started successfully!", type='positive')
        except Exception as e:
            ui.notify(f"Error starting workflow: {str(e)}", type='negative')
            print(f"Error details: {e}")

    def show_logs(self):
        logs = self.workflow.get('logs', [])
        if logs:
            ui.label("Logs").classes("text-h6")
            ui.textarea(value="\n".join(logs)).classes("w-full").props("rows=10")

