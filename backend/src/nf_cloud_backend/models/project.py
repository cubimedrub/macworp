# std imports
from __future__ import annotations
import json
from pathlib import Path
import re
import shutil
from typing import IO
from typing_extensions import Buffer

# 3rd party imports
from peewee import BigAutoField, \
    CharField, \
    BooleanField, \
    IntegerField, \
    BigIntegerField
from playhouse.postgres_ext import BinaryJSONField

# internal import
from nf_cloud_backend import db_wrapper as db
from nf_cloud_backend.utility.configuration import Configuration


SLASH_SEQ_REGEX: re.Pattern = re.compile(r"\/+")
"""Regex to match multiple sequence ofg slashes.
"""

class Project(db.Model):
    id = BigAutoField(primary_key=True)
    name = CharField(max_length=512, null=False)
    workflow_id = BigIntegerField(null=False, default=0)
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
    def file_directory(self) -> Path:
        """
        Returns the project's file/work directory.

        Returns
        -------
        Path
            Root path of the project's file directory.
        """
        return self.__file_directory

    def __create_file_directory(self):
        self.__file_directory = Path(Configuration.values()["upload_path"]).joinpath(str(self.id)).absolute()
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


    def to_dict(self) -> dict:
        """
        Returns
        -------
        Returns the project as JSON
        """
        return {
            "id": self.id,
            "name": self.name,
            "workflow_arguments": self.workflow_arguments,
            "workflow_id": self.workflow_id,
            "submitted_processes": self.submitted_processes,
            "completed_processes": self.completed_processes,
            "is_scheduled": self.is_scheduled
        }

    def __secure_path_for_join(self, path: Path) -> Path:
        """
        Removes dangerous directory operations from path, e.g.
        * `/` at path begin would result in a absolut path escaping the project directory
        * `..` can be used to escape the project dir.

        Parameters
        ----------
        path : Path
            Path to join with project dir.s

        Returns
        -------
        Path
            Relative path, secure to join with projects directory.
        """
        parts = list(path.parts)

        # Remove leading slashes to avoid overwriting the projects directory when using joinpath
        while True:
            if len(parts) == 0 or SLASH_SEQ_REGEX.match(parts[0]) is None:
                break
            parts = parts[1:]

        # Remove all `..` from path as they could be used to escape the project directory
        parts = list(filter(lambda x: x != "..", parts))

        return Path(*parts)

    def in_file_director(self, path: Path) -> bool:
        """
        Checks if the given path is part of the project's file directory.

        Parameters
        ----------
        path : Path
            Path to check (absolute)

        Returns
        -------
        bool
            True if path if part of file directory or file directory itself.
        """
        return self.file_directory == path or self.file_directory in path.parents
    
    def get_path(self, path: Path) -> Path:
        """
        Adds the given path to the project's file directory. If the joined path results in a target
        outside project directory the project directory itself is returned.
        This method should be used to get a path within the project directory safely.

        Parameters
        ----------
        path : Path
            Path within the project directory

        Returns
        -------
        Path
            Absolute path within the project directory.

        Raises
        ------
        PermissionError
            Raised when path is outside the project directory.
        """
        directory = self.file_directory.absolute().joinpath(self.__secure_path_for_join(path))
        if self.in_file_director(directory):
            return directory
        else:
            raise PermissionError("Path is not within the projects directory.")

    def add_file(self, target_file_path: Path, file: Buffer) -> Path:
        """
        Add file to directory

        Parameters
        ----------
        target_file_path: Path
            Path of the file within the project directory
        file : IO[bytes]
            File

        Returns
        -------
        Path
            Relative path to the file within the project directory
        """
        target_directory = self.get_path(target_file_path.parent)
        if not target_directory.is_dir():
            target_directory.mkdir(parents=True, exist_ok=True)
        with target_directory.joinpath(target_file_path.name).open("wb") as project_file:
            project_file.write(file)

        return Path("/").joinpath(self.__secure_path_for_join(target_file_path))

    def add_file_chunk(
        self,
        target_file_path: Path,
        chunk_offset: int,
        file_chunk: IO[bytes]
    ) -> Path:
        """
        Adds a file_chunk to the given target_file within the project directory. 
        Useful for uploading large files.

        Parameters
        ----------
        target_file_path: Path
            Path of the file within the project directory
        chunk_offset : int
            Offset of the file chunk
        file_chunk : IO[bytes]
            File chunk

        Returns
        -------
        Path
            Relative path to the file within the project directory
        """
        target_directory = self.get_path(target_file_path.parent)
        if not target_directory.is_dir():
            target_directory.mkdir(parents=True, exist_ok=True)
        with target_directory.joinpath(target_file_path.name).open("ab") as project_file:
            project_file.seek(chunk_offset)
            project_file.write(file_chunk.read())

        return Path("/").joinpath(self.__secure_path_for_join(target_file_path))

    def remove_path(self, path: Path) -> bool:
        """
        Removes the given file from the file directory.

        Parameters
        ----------
        path : Path
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

    def create_folder(self, new_folder_path: Path) -> bool:
        """
        Creates a folder in the work directory of the project.
        Creates parents as well, if path contains multiple segments.

        Parameters
        ----------
        new_folder_path : Path
            Path where the new folder will be created.

        Returns
        -------
        True if path was created, otherwise False
        """
        new_folder_path = self.get_path(new_folder_path)

        if not new_folder_path.is_dir():
            new_folder_path.mkdir(parents=True, exist_ok=True)
            return True
        return False

    def get_queue_represenation(self) -> str:
        """
        Returns
        -------
        JSON string for message queue
        """
        if self.workflow_id <= 0:
            raise ValueError("Workflow ID is not set.")
        return json.dumps({
            "id": self.id,
            "workflow_id": self.workflow_id,
            "workflow_arguments": self.workflow_arguments
        })