"""Object and functions to deal with project data"""

from __future__ import annotations
import argparse
from pathlib import Path
import shutil
from typing import IO
from typing_extensions import Buffer

from macworp_utils.exchange.queued_project import QueuedProject
from macworp_utils.path import is_within_path, secure_joinpath
from peewee import BigAutoField, CharField, BooleanField, IntegerField, BigIntegerField
from playhouse.postgres_ext import BinaryJSONField

from macworp_backend import db_wrapper as db
from macworp_backend.utility.configuration import Configuration


class Project(db.Model):  # type: ignore[name-defined]
    """Handling project data"""

    id = BigAutoField(primary_key=True)
    name = CharField(max_length=512, null=False)
    workflow_id = BigIntegerField(null=False, default=0)
    workflow_arguments = BinaryJSONField(null=False, default={})
    is_scheduled = BooleanField(null=False, default=False)
    submitted_processes = IntegerField(null=False, default=0)
    completed_processes = IntegerField(null=False, default=0)
    ignore = BooleanField(null=False, default=False)

    class Meta:
        """Peewee meta class"""

        db_table = "projects"

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
        self.__file_directory = (
            Path(Configuration.values()["upload_path"])
            .joinpath(str(self.id))
            .absolute()
        )
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
            Delete related models that have a null foreign key.
            If False nullable relations will be set to NULL
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
            "is_scheduled": self.is_scheduled,
            "ignore": self.ignore,
        }

    def in_file_directory(self, path: Path) -> bool:
        """
        Checks if the given path is part of the project's file directory.

        Parameters
        ----------
        path : Path
            Path to check

        Returns
        -------
        bool
            True if path is part of the project's file directory
        """
        return is_within_path(self.file_directory.absolute(), path)

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
        absolute_fire_directory = self.file_directory.absolute()
        directory = secure_joinpath(absolute_fire_directory, path)
        if self.in_file_directory(directory):
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
        with target_directory.joinpath(target_file_path.name).open(
            "wb"
        ) as project_file:
            project_file.write(file)

        return Path("/").joinpath(self.__secure_path_for_join(target_file_path))

    def add_file_chunk(
        self, target_file_path: Path, chunk_offset: int, file_chunk: IO[bytes]
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
        with target_directory.joinpath(target_file_path.name).open(
            "ab"
        ) as project_file:
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

    def get_queue_representation(self) -> QueuedProject:
        """
        Returns
        -------
        JSON string for message queue
        """
        if self.workflow_id <= 0:
            raise ValueError("Workflow ID is not set.")
        return QueuedProject(
            id=self.id,  # type: ignore[arg-type]
            workflow_id=self.workflow_id,  # type: ignore[arg-type]
            workflow_arguments=self.workflow_arguments,  # type: ignore[arg-type]
        )


class ProjectCommandLineInterface:
    """Command line interface for projects"""

    @classmethod
    def set_project_ignore(cls, project_id: int, ignore: bool):
        """
        Set project ignore flag to a new value.
        If ignore is set to True, the project schedule status is also reset.

        Parameters
        ----------
        project_id : int
            Project ID
        ignore : bool
            Ignore flag
        """
        project = Project.get(Project.id == project_id)
        project.ignore = ignore
        if ignore:
            project.is_scheduled = False
            project.submitted_processes = 0
            project.completed_processes = 0
        project.save()

    @classmethod
    def add_set_project_ignore_cli(cls, subparsers: argparse._SubParsersAction):
        """
        Adds CLI for setting or unsetting the ignore flag

        Parameters
        ----------
        subparsers : argparse._SubParsersAction
            Subparser for new arguments
        """

        parser = subparsers.add_parser(
            "ignore",
            help=(
                "Sets or unset the ignore flag. "
                "A ignored project is removed from the queue once a worker is receiving it."
            ),
        )
        parser.set_defaults(func=lambda args: parser.print_help())

        parser.add_argument("project_id", type=int, help="Project ID")
        group = parser.add_mutually_exclusive_group(required=True)
        group.add_argument(
            "--set",
            action="store_true",
            help="Set the ignore flag and removes the projects schedule status",
        )
        group.add_argument("--unset", action="store_true", help="Unset the ignore flag")

        def set_project_ignore(cli_args: argparse.Namespace):
            """Determines the new ignore flag and calls the set_project_ignore method"""
            ignore = True
            if cli_args.set:
                ignore = True
            elif cli_args.unset:
                ignore = False
            cls.set_project_ignore(cli_args.project_id, ignore)

        parser.set_defaults(func=set_project_ignore)

    @classmethod
    def add_cli_arguments(cls, subparsers: argparse._SubParsersAction):
        """
        Adds cli arguments for projects

        Parameters
        ----------
        subparsers : argparse._SubParsersAction
            Subparser for new arguments
        """
        parser = subparsers.add_parser("projects", help="Project utilities")
        parser.set_defaults(func=lambda args: parser.print_help())

        local_subparsers: argparse._SubParsersAction = parser.add_subparsers()
        cls.add_set_project_ignore_cli(local_subparsers)
