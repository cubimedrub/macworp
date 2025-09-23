from typing import Dict, List, Any, Optional


class WorkflowParameterService:
    """Service for handling workflow parameter validation and transformation."""

    def __init__(self):
        self._uploaded_files: Dict[str, List[str]] = {}

    def store_uploaded_file(self, param_id: str, filename: str) -> None:
        """
        Store an uploaded filename for a parameter.

        Args:
            param_id (str): Parameter identifier
            filename (str): Name of an uploaded file

        Returns:
            None
        """
        if param_id not in self._uploaded_files:
            self._uploaded_files[param_id] = []
        self._uploaded_files[param_id].append(filename)

    def get_uploaded_files(self, param_id: str) -> Optional[List[str]]:
        """
        Get uploaded files for a parameter.

        Args:
            param_id (str): Parameter identifier

        Returns:
            Optional[List[str]]: List of uploaded filenames or None
        """
        return self._uploaded_files.get(param_id)

    def clear_uploaded_files(self, param_id: Optional[str] = None) -> None:
        """
        Clear uploaded files for a parameter or all parameters.

        Args:
            param_id (Optional[str]): Specific parameter ID to clear, or None for all

        Returns:
            None
        """
        if param_id:
            self._uploaded_files.pop(param_id, None)
        else:
            self._uploaded_files.clear()

    def collect_parameter_values(
        self, parameter_inputs: Dict[str, Any], param_mapping: Dict[str, Dict]
    ) -> Dict[str, Any]:
        """
        Collect parameter values from input elements.

        Args:
            parameter_inputs (Dict[str, Any]): Mapping of parameter IDs to UI elements
            param_mapping (Dict[str, Dict]): Parameter definitions

        Returns:
            Dict[str, Any]: Dictionary mapping parameter IDs to their values
        """
        user_parameter_values = {}

        for param_id, input_element in parameter_inputs.items():
            param_def = param_mapping.get(param_id, {})
            param_type = param_def.get("type")
            value = None

            if param_type in ("path", "paths"):
                files = self._uploaded_files.get(param_id)
                if files:
                    value = files if len(files) > 1 else files[0]
            else:
                if hasattr(input_element, "value"):
                    value = input_element.value

            user_parameter_values[param_id] = value

        return user_parameter_values

    def convert_to_workflow_format(
        self, user_parameter_values: Dict[str, Any], param_mapping: Dict[str, Dict]
    ) -> List[Dict[str, Any]]:
        """
        Convert user parameter values to workflow parameter format.

        Args:
            user_parameter_values (Dict[str, Any]): Raw parameter values
            param_mapping (Dict[str, Dict]): Parameter definitions

        Returns:
            List[Dict[str, Any]]: List of parameter dictionaries in workflow format
        """
        workflow_parameters = []
        for param_id, value in user_parameter_values.items():
            param_def = param_mapping.get(param_id, {})
            workflow_parameters.append(
                {
                    "name": param_def.get("name", param_id),
                    "label": param_def.get("label", param_id),
                    "type": param_def.get("type"),
                    "value": value,
                }
            )

        return workflow_parameters

    def validate_required_parameters(
        self, workflow_parameters: List[Dict[str, Any]], param_mapping: Dict[str, Dict]
    ) -> List[str]:
        """
        Validate that all required parameters have values.

        Args:
            workflow_parameters (List[Dict[str, Any]]): Workflow parameter dictionaries
            param_mapping (Dict[str, Dict]): Parameter definitions

        Returns:
            List[str]: List of missing required parameter labels
        """
        missing_params = []
        for param_id, param_def in param_mapping.items():
            param_label = param_def.get("label", param_id)
            if param_label in [p["label"] for p in workflow_parameters]:
                value = next(
                    p["value"] for p in workflow_parameters if p["label"] == param_label
                )
                if self._is_empty_value(value):
                    missing_params.append(param_label)

        return missing_params

    def _is_empty_value(self, value: Any) -> bool:
        """
        Check if a parameter value is considered empty.

        Args:
            value (Any): The value to check

        Returns:
            bool: True if the value is empty, False otherwise
        """
        return (
            value is None
            or value == ""
            or (isinstance(value, list) and len(value) == 0)
        )
