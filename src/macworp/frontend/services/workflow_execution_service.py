from typing import Dict, List, Any


class WorkflowExecutionService:
    """Service for executing workflows."""

    def __init__(self, workflows_service):
        self._workflows_service = workflows_service

    async def start_workflow_with_parameters(
        self,
        project_id: int,
        workflow: Dict,
        workflow_parameters: List[Dict[str, Any]]
    ) -> Dict[str, Any]:
        """
        Start a workflow with the given parameters.

        Args:
            project_id (int): ID of the project
            workflow (Dict): Workflow definition
            workflow_parameters (List[Dict[str, Any]]): List of parameter dictionaries

        Returns:
            Dict[str, Any]: Result of workflow execution

        Raises:
            Exception: If workflow execution fails
        """
        try:
            result = await self._workflows_service.start_workflow(
                project_id,
                workflow,
                workflow_parameters
            )
            return {
                'success': True,
                'message': 'Workflow started successfully!',
                'result': result
            }
        except Exception as e:
            return {
                'success': False,
                'message': f'Error starting workflow: {str(e)}',
                'error': str(e)
            }

    def get_workflow_logs(self, workflow: Dict) -> List[str]:
        """
        Extract logs from a workflow.

        Args:
            workflow (Dict): Workflow object

        Returns:
            List[str]: List of log messages
        """
        return workflow.get('logs', [])
