
from typing import Dict, List, Any, Optional

from nicegui import ui

from frontend.services.workflow_execution_service import WorkflowExecutionService
from frontend.services.workflow_parameter_service import WorkflowParameterService
from frontend.services.workflow_service import WorkflowService


class WorkflowStart:
    """
    Handles UI interactions for starting and managing workflows in a project.

    This class is responsible only for UI rendering and user interaction,
    while business logic is delegated to service classes.
    """

    def __init__(self, project_id: int):
        self.project_id = project_id
        self.workflow: Optional[Dict] = None
        self.workflows_service = WorkflowService()
        self.parameter_service = WorkflowParameterService()
        self.execution_service = WorkflowExecutionService(self.workflows_service)

        # UI-specific state
        self.workflows: List[Dict] = []
        self.parameter_inputs: Dict[str, Any] = {}
        self.param_mapping: Dict[str, Dict] = {}
        self.dialog_content: Optional[Any] = None

    async def get_workflows(self) -> List[Dict]:
        """Load workflows for the current project.

        Returns:
            List[Dict]: List of workflows
        """
        self.workflows = await self.workflows_service.load_workflows(project_id=self.project_id)
        return self.workflows

    async def workflow_selection(self) -> None:
        """
        Show selection and creation dialog for workflows.

        Returns:
            None
        """
        await self.get_workflows()

        with ui.dialog().props('backdrop-filter="blur(8px) brightness(40%)"') as dialog, ui.card().classes('w-full'):
            self.dialog_content = ui.column()

            with self.dialog_content:
                with ui.dropdown_button('Workflows', auto_close=True):
                    for workflow in self.workflows:
                        ui.item(workflow["name"] or workflow["id"],
                                on_click=lambda w=workflow: self._show_workflow_params(w))

            ui.button("New Workflow")

        await dialog

    async def _show_workflow_params(self, selected_workflow: Dict) -> None:
        """
        Display the UI for workflow parameters and handle input.

        Args:
            selected_workflow (Dict): Workflow object selected by the user

        Returns:
            None
        """
        self.workflow = selected_workflow
        self.dialog_content.clear()

        workflow = await self.workflows_service.get_workflow(selected_workflow)
        params = workflow.get('definition', {}).get('parameters', {}).get('dynamic', [])

        # Clear previous state
        self.parameter_inputs.clear()
        self.param_mapping.clear()
        self.parameter_service.clear_uploaded_files()

        await self._render_parameter_inputs(selected_workflow, params)

    async def _render_parameter_inputs(self, selected_workflow: Dict, params: List[Dict]) -> None:
        """
        Render input elements for workflow parameters.

        Args:
            selected_workflow (Dict): Selected workflow
            params (List[Dict]): List of parameter definitions

        Returns:
            None
        """
        with self.dialog_content:
            ui.label(f"Workflow: {selected_workflow['name']}").classes('text-h5')

            for param in params:
                await self._render_single_parameter(param)

            ui.button("Delete Workflow",
                      on_click=lambda w=selected_workflow: self.workflows_service.delete_workflow(w))
            ui.button("Start Workflow", on_click=self._start_workflow_with_params)

    async def _render_single_parameter(self, param: Dict) -> None:
        """
        Render a single parameter input element.

        Args:
            param (Dict): Parameter definition

        Returns:
            None
        """
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

        elif param_type in ("path", "paths"):
            await self._render_file_upload(param, param_id)

    async def _render_file_upload(self, param: Dict, param_id: str) -> None:
        """
        Render file upload element for path/paths parameters.

        Args:
            param (Dict): Parameter definition
            param_id (str): Parameter ID

        Returns:
            None
        """
        desc = param.get("desc", "")
        multiple = param["type"] == "paths" or ("folder" in desc.lower()) or ("Multiple" in desc)

        input_element = ui.upload(
            multiple=multiple,
            auto_upload=True,
            label=param["label"],
            on_upload=lambda e, pid=param_id: self._store_uploaded_files(pid, e)
        )

        upload_id = f"upload_{param_id.replace(' ', '_')}"
        input_element._props['id'] = upload_id

        if "folder" in desc.lower():
            await self._configure_folder_upload(upload_id)
        elif multiple:
            await self._configure_multiple_upload(upload_id)

        self.parameter_inputs[param_id] = input_element

    async def _configure_folder_upload(self, upload_id: str) -> None:
        """
        Configure upload element for folder selection.

        Args:
            upload_id (str): DOM ID of upload element

        Returns:
            None
        """
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

    async def _configure_multiple_upload(self, upload_id: str) -> None:
        """
        Configure upload element for multiple file selection.

        Args:
            upload_id (str): DOM ID of upload element

        Returns:
            None
        """
        await ui.run_javascript(f'''
            const uploadElement = document.getElementById('{upload_id}');
            if (uploadElement) {{
                const input = uploadElement.querySelector('input[type="file"]');
                if (input && !input.hasAttribute('multiple')) {{
                    input.setAttribute('multiple', '');
                }}
            }}
        ''')

    def _store_uploaded_files(self, param_id: str, event: Any) -> None:
        """
        Store uploaded file names for a parameter.

        Args:
            param_id (str): The parameter identifier
            event (Any): The event object containing the uploaded file name

        Returns:
            None
        """
        self.parameter_service.store_uploaded_file(param_id, event.name)

    async def _start_workflow_with_params(self) -> None:
        """
        Start a workflow with the collected user parameters.

        Returns:
            None
        """
        # Collect and validate parameters using service
        user_parameter_values = self.parameter_service.collect_parameter_values(
            self.parameter_inputs,
            self.param_mapping
        )

        workflow_parameters = self.parameter_service.convert_to_workflow_format(
            user_parameter_values,
            self.param_mapping
        )

        missing_params = self.parameter_service.validate_required_parameters(
            workflow_parameters,
            self.param_mapping
        )

        if missing_params:
            ui.notify(f"Missing required parameters: {', '.join(missing_params)}", type='negative')
            return

        # Execute workflow using service
        result = await self.execution_service.start_workflow_with_parameters(
            self.project_id,
            self.workflow,
            workflow_parameters
        )

        # Handle UI feedback
        ui.notify(result['message'], type='positive' if result['success'] else 'negative')
        if not result['success']:
            print(f"Error details: {result.get('error', 'Unknown error')}")

    def show_logs(self) -> None:
        """
        Display workflow logs in the user interface.

        Returns:
            None
        """
        if not self.workflow:
            return

        logs = self.execution_service.get_workflow_logs(self.workflow)
        if logs:
            ui.label("Logs").classes("text-h6")
            ui.textarea(value="\n".join(logs)).classes("w-full").props("rows=10")
