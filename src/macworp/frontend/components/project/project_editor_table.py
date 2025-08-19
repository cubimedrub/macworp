from typing import Any

from nicegui import ui

from macworp.configuration import Configuration
from macworp.frontend.services.project_service import ProjectService


class ProjectEditTable:
    """Component for creating or editing projects via an editable dialog table."""

    def __init__(self, project, project_id, config: Configuration, auth_token: str):
        self.project = project
        self.project_service = ProjectService(config, auth_token)
        self.project_id = project_id

    async def create_editable_table(self) -> None:
        """
        Create and display an editable project table in a dialog.

        Loads project data if editing an existing project or initializes a new
        project template. Allows editing of fields including strings, numbers,
        booleans, lists, and dictionaries.

        Returns:
            None
        """

        # Check if this is for creating a new project
        is_new_project = self.project_id is None and self.project is None
        # Load project data if editing existing project
        if not is_new_project and not self.project:
            try:
                self.project = await self.project_service.load_project(self.project_id)
            except Exception as e:
                ui.notify(f"Error loading project: {str(e)}", color="negative")
                return

        # For new projects, create empty template
        if is_new_project:
            self.project = self._get_project_template()

        if not self.project:
            ui.notify("Could not load or create project data", color="negative")
            return

        dialog_title = "Create New Project" if is_new_project else "Edit Project"

        with (
            ui.dialog().props('backdrop-filter="blur(8px) brightness(40%)"') as dialog,
            ui.card().classes("w-full"),
        ):
            ui.label(dialog_title).classes("text-h6 mb-4 center")

            with ui.column(wrap=True).classes("w-full gap-4 border p-1"):
                for key, value in self.project.items():
                    if key in [
                        "id",
                        "created_at",
                        "updated_at",
                        "workflow_id",
                        "workflow_arguments"
                    ]:  # Skip non-editable fields
                        continue

                    ui.label(key.replace("_", " ").title()).classes(
                        "text-left font-medium"
                    )

                    # "ispublished" checkbox
                    if isinstance(value, bool):
                        ui.checkbox(value=value).bind_value_to(self.project, key)
                    # share with users
                    elif isinstance(value, list):
                        list_str = (
                            ", ".join(str(item) for item in value) if value else ""
                        )
                        list_input = ui.input(
                            value=list_str, placeholder="Comma-separated values"
                        )
                        list_input.on(
                            "blur",
                            lambda e, k=key: self._update_list_field(k, e.sender.value),
                        )
                    # workflow arguments
                    elif isinstance(value, dict):
                        dict_input = ui.textarea(
                            value=str(value), placeholder="JSON format"
                        )
                        dict_input.on(
                            "blur",
                            lambda e, k=key: self._update_dict_field(k, e.sender.value),
                        )
                    elif value is None:
                        ui.input(value="", placeholder="Empty").bind_value_to(
                            self.project, key
                        )
                    # name and description
                    else:
                        ui.input(value=str(value)).bind_value_to(self.project, key)

            save_button_text = "Create Project" if is_new_project else "Save Changes"
            ui.button(
                save_button_text,
                color="primary",
                on_click=lambda: self.save_project(dialog, is_new_project),
            )

        await dialog

    def _get_project_template(self) -> dict[str, str | None | dict[Any, Any] | bool]:
        """
        Return a default template for a new project.

        Returns:
            dict: A dictionary with default project fields.
        """
        return {
            "name": "",
            "description": "",
            "is_published": False,
        }

    def _update_list_field(self, key: str, value_str: str) -> None:
        """
        Update a list-type field from a comma-separated string.

        Args:
            key (str): The project field to update.
            value_str (str): Comma-separated string representing list items.

        Returns:
            None
        """
        if value_str.strip():
            self.project[key] = [
                item.strip() for item in value_str.split(",") if item.strip()
            ]
        else:
            self.project[key] = []

    def _update_dict_field(self, key: str, value_str: str):
        """
        Update a dict-type field from a JSON string.

        Args:
            key (str): The project field to update.
            value_str (str): JSON string representing the dictionary.

        Returns:
            None
        """
        try:
            import json
            if value_str.strip():
                self.project[key] = json.loads(value_str)
            else:
                self.project[key] = {}
        except:
            ui.notify(f'Invalid JSON format for {key}', color='warning')

    def _validate_project_before_save(self) -> list[str]:
        """
        Validate project data before saving.

        Checks for required fields and field length constraints.

        Returns:
            list[str]: A list of error messages. Empty if project is valid.
        """
        errors = []

        # Check name
        name = self.project.get("name", "")
        if not name or name == "":
            errors.append("Name cannot be empty")
        elif len(name) > 100:
            errors.append("Name too long (max 100 characters)")

        # Check description
        description = self.project.get("description", "")
        if not description or description == "":
            errors.append("Description cannot be empty")
        elif len(description) > 100:
            errors.append("Description too long (max 100 characters)")

        return errors

    async def save_project(self, dialog, is_new_project: bool) -> None:
        """
        Save the current project.

        Handles both creation of new projects and updates to existing projects.

        Args:
            dialog: The NiceGUI dialog object to close after saving.
            is_new_project (bool): True if creating a new project, False if updating.

        Returns:
            None
        """
        try:
            validation_errors = self._validate_project_before_save()
            if validation_errors:
                error_message = "Please fix these errors:\n" + "\n".join(
                    validation_errors
                )
                ui.notify(error_message, color="negative")
                return  # Don't save if validation fails

            if is_new_project:
                # Clean up data before sending
                project_data = self.project.copy()

                # Convert workflow_id: empty string -> None, valid string -> int
                workflow_id = project_data.get("workflow_id")
                if workflow_id == "" or workflow_id is None:
                    project_data["workflow_id"] = None
                elif isinstance(workflow_id, str):
                    try:
                        project_data["workflow_id"] = int(workflow_id)
                    except ValueError:
                        project_data["workflow_id"] = None

                # Ensure workflow_arguments is a dict
                if isinstance(project_data.get("workflow_arguments"), str):
                    try:
                        import json

                        project_data["workflow_arguments"] = (
                            json.loads(project_data["workflow_arguments"])
                            if project_data["workflow_arguments"].strip()
                            else {}
                        )
                    except:
                        project_data["workflow_arguments"] = {}

                # Create new project
                new_project = await self.project_service.create_project(project_data)
                self.project_id = (
                    new_project
                    if isinstance(new_project, int)
                    else new_project.get("id")
                )
                ui.notify("Project created successfully!", color="positive")
            else:
                # Update existing project
                await self.project_service.update_project(self.project, self.project_id)
                ui.notify("Project saved successfully!", color="positive")

            dialog.close()

        except Exception as e:
            ui.notify(f"Error saving project: {str(e)}", color="negative")
