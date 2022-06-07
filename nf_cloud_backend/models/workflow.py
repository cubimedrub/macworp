# std imports
from __future__ import annotations
import io
import json
import pathlib
import shutil
from typing import Union

# 3rd party imports
from nf_cloud_backend import config

class Workflow:
    TABLE_NAME = "workflows"
    def __init__(self, id: int, name: str, nextflow_workflow: str = "", nextflow_arguments: dict = {}, is_scheduled: bool = False, submitted_processes: int = 0, completed_processes: int = 0):
        self.__id = id
        self.__name = name
        self.__file_directory = None
        self.__nextflow_workflow = nextflow_workflow
        self.__nextflow_arguments = nextflow_arguments
        self.__is_scheduled = is_scheduled
        self.__submitted_processes = submitted_processes
        self.__completed_tasks = completed_processes
        self.__create_file_directory()

    @property
    def id(self) -> int:
        return self.__id

    @property
    def name(self) -> str:
        return self.__name

    @property
    def file_directory(self) -> pathlib.Path:
        return self.__file_directory

    @property
    def nextflow_workflow(self) -> str:
        return self.__nextflow_workflow

    @nextflow_workflow.setter
    def nextflow_workflow(self, value: str):
        self.__nextflow_workflow = value

    @property
    def nextflow_arguments(self) -> dict:
        return self.__nextflow_arguments

    @nextflow_arguments.setter
    def nextflow_arguments(self, value: dict):
        self.__nextflow_arguments = value

    @property
    def is_scheduled(self) -> bool:
        return self.__is_scheduled

    @is_scheduled.setter
    def is_scheduled(self, value: bool):
        self.__is_scheduled = value

    @property
    def submitted_processes(self) -> int:
        return self.__submitted_processes

    @submitted_processes.setter
    def submitted_processes(self, value: int):
        self.__submitted_processes = value

    @property
    def completed_processes(self) -> int:
        return self.__completed_tasks

    @completed_processes.setter
    def completed_processes(self, value: int):
        self.__completed_tasks = value

    def __create_file_directory(self):
        self.__file_directory = pathlib.Path(config["upload_path"]).joinpath(str(self.id))
        self.__file_directory.mkdir(parents=True, exist_ok=True)

    def __delete_file_directory(self):
        shutil.rmtree(self.file_directory)
        self.__file_directory = None


    def to_dict(self) -> str:
        """
        Returns
        -------
        Returns the workflow as JSON
        """
        return {
            "id": self.id,
            "name": self.name,
            "nextflow_arguments": self.nextflow_arguments,
            "nextflow_workflow": self.nextflow_workflow,
            "submitted_processes": self.submitted_processes,
            "completed_processes": self.completed_processes,
            "is_scheduled": self.is_scheduled
        }

    def insert(self, database_cursor) -> bool:
        """
        Inserts the workflow if the workflow has no ID

        Parameters
        ----------
        database_cursor : Cursor
            database cursor
        
        Returns
        -------
        Returns if the insertion was successfull.
        """
        if self.id == None:
            INSERT_QUERY = f"INSERT INTO {self.__class__.TABLE_NAME} (name, nextflow_workflow, nextflow_arguments, is_scheduled, submitted_processes, completed_processes) VALUES (%s, %s, %s, %s, %s, %s) RETURNING ID;"
            database_cursor.execute(
                INSERT_QUERY,
                (
                    self.name,
                    self.nextflow_workflow,
                    json.dumps(self.nextflow_arguments),
                    self.is_scheduled,
                    self.submitted_processes,
                    self.completed_processes
                )
            )
            row = database_cursor.fetchone()
            if row:
                self.__id = row[0]
                return True
        return False

    def update(self, database_cursor) -> bool:
        """
        Updates the workflow if the workflow has a ID

        Parameters
        ----------
        database_cursor : Cursor
            database cursor
        
        Returns
        -------
        Returns if the update was successfull.
        """
        if self.id != None:
            INSERT_QUERY = f"UPDATE {self.__class__.TABLE_NAME} SET name = %s, nextflow_workflow = %s, nextflow_arguments = %s, is_scheduled = %s, submitted_processes = %s, completed_processes = %s WHERE id = %s;"
            database_cursor.execute(
                INSERT_QUERY,
                (
                    self.name,
                    self.nextflow_workflow,
                    json.dumps(self.nextflow_arguments),
                    self.is_scheduled,
                    self.submitted_processes,
                    self.completed_processes,
                    self.id
                )
            )
            return True
        return False

    def delete(self, database_cursor) -> bool:
        """
        Deletes the workflow if the workflow has a ID

        Parameters
        ----------
        database_cursor : Cursor
            database cursor
        
        Returns
        -------
        Returns if the deletion was successfull.
        """
        if self.id != None:
            DELETE_QUERY = f"DELETE FROM {self.__class__.TABLE_NAME} WHERE id = %s;"
            database_cursor.execute(
                DELETE_QUERY,
                (
                    self.id,
                )
            )
            self.__id = None
            self.__delete_file_directory()
            return True
        return False

    def add_file(self, directory: str, filename: str, file: Union[io.BytesIO, io.StringIO]):
        if len(directory) > 0 and directory[0] == "/":
            directory = directory[1:]

        target_directory = self.file_directory.joinpath(directory)
        if not target_directory.is_dir():
            target_directory.mkdir(parents=True, exist_ok=True)
        with target_directory.joinpath(filename).open("wb") as workflow_file:
            workflow_file.write(file)

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
        if len(path) > 0 and path[0] == "/":
            path = path[1:]
        full_path = self.file_directory.joinpath(path)
        if full_path.is_file():
            full_path.unlink()
            return True
        elif full_path.is_dir():
            shutil.rmtree(full_path)
            return True
        return False

    def create_folder(self, target_path: str, new_path: str) -> bool:
        """
        Creates a folder in the work directory of the workflow.
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
        if len(target_path) > 0 and target_path[0] == "/":
            target_path = target_path[1:]
        if len(new_path) > 0 and new_path[0] == "/":
            new_path = new_path[1:]

        path_to_create = self.file_directory.joinpath(target_path).joinpath(new_path)

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
            "nextflow_workflow": self.nextflow_workflow,
            "nextflow_arguments": self.nextflow_arguments
        })


    #### Class methods ####
    @classmethod
    def select(cls, database_cursor, condition: str, condition_values: list, offset: int = None, limit: int = None, order_by: str = None, order_direction: str = None, fetchall: bool = False):
        """
        Parameters
        ----------
        database_cursor : Cursor
            database cursor
        condition : str
            Part of the sql query after WHERE
        condition_vaues : list
            List of values which substitute the %s in the conditions
        offset : int
            Value for OFFSET
        limit : int
            Value for LIMIT
        fetchall : bool
            Controlls if only one record is fetched or all

        Returns
        -------
        Workflow or list of workflows
        """
        select_query = f"SELECT id, name, nextflow_workflow, nextflow_arguments, is_scheduled, submitted_processes, completed_processes FROM {cls.TABLE_NAME}"
        if len(condition):
            select_query += f" WHERE {condition}"
        if offset:
            select_query += f" OFFSET {offset}"
        if limit:
            select_query += f" LIMIT {limit}"
        if order_by and order_direction:
            select_query += f" ORDER BY {order_by} {order_direction}"
        select_query += ";"
        database_cursor.execute(select_query, condition_values)
        
        if fetchall:
            return [cls(row[0], row[1], row[2], row[3], row[4], row[5], row[6]) for row in database_cursor.fetchall()]
        else:
            row = database_cursor.fetchone()
            if row:
                return cls(row[0], row[1], row[2], row[3], row[4], row[5], row[6])
            else:
                return None

    @classmethod
    def count(cls, database_cursor) -> int:
        """
        Parameter
        ---------
        database_cursor : Cursor
            Database cursor

        Return
        ------
        Returns the number of workflows in the database
        """
        COUNT_QUERY = f"SELECT count(*) FROM {cls.TABLE_NAME};"
        database_cursor.execute(COUNT_QUERY)
        return database_cursor.fetchone()[0]
