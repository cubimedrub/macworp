# std imports
from __future__ import annotations
import io
import json
import pathlib
import shutil
from typing import Union

# 3rd party imports
from peewee import BigAutoField, \
    CharField, \
    BooleanField, \
    IntegerField
from playhouse.postgres_ext import BinaryJSONField

# internal import
from nf_cloud_backend import db_wrapper as db
from nf_cloud_backend.utility.configuration import Configuration

class Project(db.Model):
    id = BigAutoField(primary_key=True)
    name = CharField(max_length=512, null=False)
    workflow = CharField(max_length=255, null=False, default="")
    workflow_arguments = BinaryJSONField(null=False, default={})
    is_scheduled = BooleanField(null=False, default=False)
    submitted_processes = IntegerField(null=False, default=0)
    completed_processes = IntegerField(null=False, default=0)

    class Meta:
        db_table="projects"

    def __init__(self, **kwargs):
        super().__init__(**kwargs)
        self.__file_directory = None
        self.__create_file_directory()

    @property
    def file_directory(self) -> pathlib.Path:
        return self.__file_directory

    def __create_file_directory(self):
        self.__file_directory = pathlib.Path(Configuration.values()["upload_path"]).joinpath(str(self.id)).absolute()
        self.__file_directory.mkdir(parents=True, exist_ok=True)

    def __delete_file_directory(self):
        shutil.rmtree(self.file_directory)
        self.__file_directory = None

    def delete_instance(self, recursive=False, delete_nullable=False):
        """
        Overrides the original delete_instance.
        Removes the file directory if a row was deleted.

        Parameters
        ----------
        recursive : bool, optional
            Delete related models.
        delete_nullable : bool, optional
            Delete related models that have a null foreign key. If False nullable relations will be set to NULL
        """
        deleted_rows = super().delete_instance(recursive=False, delete_nullable=False)
        if deleted_rows > 0:
            self.__delete_file_directory()


    def to_dict(self) -> str:
        """
        Returns
        -------
        Returns the project as JSON
        """
        return {
            "id": self.id,
            "name": self.name,
            "workflow_arguments": self.workflow_arguments,
            "workflow": self.workflow,
            "submitted_processes": self.submitted_processes,
            "completed_processes": self.completed_processes,
            "is_scheduled": self.is_scheduled
        }

    def __secure_path_for_join(self, path: str) -> str:
        """
        Removes dangerous directory operations from path, e.g.
        * `/` at path begin would result in a absolut path escaping the project directory
        * `..` can be used to escape the project dir.

        Parameters
        ----------
        path : str
            Path to join with project dir.s

        Returns
        -------
        str
            Save path
        """
        if len(path) > 0 and path[0] == "/":
            return path[1:]
        return path.replace("../", "")

    def in_file_director(self, path: pathlib.Path) -> bool:
        """
        Checks if the given path is part of the project's file directory.

        Parameters
        ----------
        path : pathlib.Path
            Path to check (absolute)

        Returns
        -------
        bool
            True if path if part of file directory or file directory itself.
        """
        return self.file_directory == path or self.file_directory in path.parents
    
    def get_path(self, path: str) -> pathlib.Path:
        """
        Adds the given path to the project's file directory. If the joined path results in a target
        outside project directory the project directory itself is returned.

        Parameters
        ----------
        path : str
            Path within the project directory

        Returns
        -------
        Path
            Absolute path within the project directory.

        Raises
        ------
        PermissionError
            Raised when path is outside the project directorsy.
        """
        directory = self.file_directory.absolute().joinpath(self.__secure_path_for_join(path))
        if self.in_file_director(directory):
            return directory
        else:
            raise PermissionError("Path is not within the project directory.")

    def add_file(self, directory: str, filename: str, file: Union[io.BytesIO, io.StringIO]):
        """
        Add file to directory

        Parameters
        ----------
        directory : str
            Target directory
        filename : str
            Filename
        file : Union[io.BytesIO, io.StringIO]
            File
        """
        target_directory = self.get_path(directory)
        if not target_directory.is_dir():
            target_directory.mkdir(parents=True, exist_ok=True)
        with target_directory.joinpath(filename).open("wb") as project_file:
            project_file.write(file)

    def remove_path(self, path: str) -> bool:
        """
        Removes the given file from the file directory.

        Parameters
        ----------
        path : str
            File or folder path. If path ends with a slash it is a directory.

        Returns
        -------
        Returns true (file was deleted) or false (file does not exists) 
        """
        full_path = self.get_path(path)
        if full_path.is_file():
            full_path.unlink()
            return True
        elif full_path.is_dir():
            shutil.rmtree(full_path)
            return True
        return False

    def create_folder(self, target_path: str, new_path: str) -> bool:
        """
        Creates a folder in the work directory of the project.
        Creates parents as well, if path contains multiple segments.

        Parameters
        ----------
        target_path : str
            Path where the new folder will be created.
        new_path : str
            Path to new folder

        Returns
        -------
        True if path was created, otherwise False
        """
        target_path = self.get_path(target_path)
        new_path = self.__secure_path_for_join(new_path)

        path_to_create = target_path.joinpath(new_path)

        if not path_to_create.is_dir():
            path_to_create.mkdir(parents=True, exist_ok=True)
            return True
        return False

    def get_queue_represenation(self) -> str:
        """
        Returns
        -------
        JSON string for message queue
        """
        return json.dumps({
            "id": self.id,
            "workflow": self.workflow,
            "workflow_arguments": self.workflow_arguments
        })