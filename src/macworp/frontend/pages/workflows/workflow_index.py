from nicegui import ui

from macworp.configuration import Configuration
from macworp.frontend.services.workflow_service import WorkflowService


class WorkflowIndex:
    """
    A class for managing and displaying workflow indexes in a UI.

    This class provides functionality to load, display, and manage workflows
    through a user interface. It handles the rendering of workflow tables
    and provides actions like deletion.

    Attributes:
        workflow_service: Service instance for workflow operations.
        workflows: List of loaded workflows.
    """

    def __init__(
        self,
        config: Configuration,
        auth_token: str,
        workflow_service: WorkflowService = None,
    ):
        """
        Initialize the WorkflowIndex.

        Args:
            workflow_service: Optional workflow service instance. If not provided,
                            a new WorkflowService instance will be created.
        """
        self.workflow_service = workflow_service or WorkflowService(config, auth_token)
        self.workflows = []
        self._workflow_list = None

    async def load_workflows(self):
        """
        Load workflows from the service and store them in self.workflows.

        This method fetches workflows from the service and updates the internal
        workflows list. It doesn't return anything as the workflows are stored
        as an instance variable.

        Raises:
            Exception: If the workflow service fails to load workflows.
        """
        self.workflows = await self.workflow_service.load_workflows() or []

    async def delete_workflow(self, workflow_id):
        """
        Delete a workflow by ID.

        Args:
            workflow_id: The ID of the workflow to delete.

        Todo:
            Implement workflow deletion functionality.
        """
        pass

    async def _render_workflow_table(self):
        """
        Render the workflow table in the UI.

        This private method handles the rendering of workflows in a table format.
        It clears existing content and rebuilds the workflow list based on
        the stored workflows.

        Note:
            This method modifies the UI directly and should only be called
            from within the show() method or other UI rendering contexts.
        """
        if self._workflow_list is None:
            self._workflow_list = ui.column().classes("w-full")

        if not self.workflows:
            self._workflow_list.clear()
            with self._workflow_list:
                ui.label("No workflows found").classes("text-center mt-3")
            return

        self._workflow_list.clear()

        for workflow in self.workflows:
            with self._workflow_list:
                workflow_name = self._get_workflow_name(workflow)

                with ui.card().classes("w-full mb-2"):
                    ui.label(workflow_name).classes(
                        "text-decoration-none fw-bold text-lg"
                    )

                    if isinstance(workflow, dict):
                        if "description" in workflow:
                            ui.label(workflow["description"]).classes(
                                "text-muted small"
                            )

                        # Check if workflow is published
                        if workflow.get("is_published", False):
                            ui.icon("lock_open")
                        else:
                            ui.icon("lock")

                    ui.button(
                        "Delete", on_click=lambda w=workflow: self.delete_workflow(w)
                    )

    def _get_workflow_name(self, workflow):
        """
        Extract the workflow name from a workflow object or dictionary.

        Args:
            workflow: The workflow object (dict or object with name attribute).

        Returns:
            str: The workflow name, or string representation if name cannot be found.
        """
        if isinstance(workflow, dict):
            return workflow.get("name", "Unnamed Workflow")
        return getattr(workflow, "name", str(workflow))

    async def show(self):
        """
        Display the workflow index UI.

        This method creates and renders the complete workflow index interface,
        including the title and the workflow table. It first loads the workflows
        and then renders the UI.
        """
        with ui.column().classes("w-full"):
            with ui.row().classes("w-full justify-between items-center mb-4"):
                ui.label("Workflows").classes("text-2xl")

            self._workflow_list = ui.column().classes("w-full")

            await self.load_workflows()
            await self._render_workflow_table()
