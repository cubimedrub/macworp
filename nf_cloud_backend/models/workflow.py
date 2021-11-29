from __future__ import annotations

import io
import json
import pathlib
import shutil
from typing import Union, List


from nf_cloud_backend import config

class Workflow:
    TABLE_NAME = "workflows"
    def __init__(self, id: int, name: str, nextflow_workflow: str = "", nextflow_workflow_type: str = "", nextflow_arguments: str = "", is_scheduled: bool = False, nextflow_log: dict = {}):
        self.__id = id
        self.__name = name
        self.__file_directory = None
        self.__nextflow_workflow = nextflow_workflow
        self.__nextflow_workflow_type = nextflow_workflow_type
        self.__nextflow_arguments = nextflow_arguments
        self.__is_scheduled = is_scheduled
        self.__nextflow_log = nextflow_log
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
    def nextflow_workflow_type(self) -> str:
        return self.__nextflow_workflow_type

    @nextflow_workflow_type.setter
    def nextflow_workflow_type(self, value: str):
        self.__nextflow_workflow_type = value


    @property
    def nextflow_arguments(self) -> str:
        return self.__nextflow_arguments

    @nextflow_arguments.setter
    def nextflow_arguments(self, value: str):
        self.__nextflow_arguments = value

    @property
    def is_scheduled(self) -> bool:
        return self.__is_scheduled

    @is_scheduled.setter
    def is_scheduled(self, value: bool):
        self.__is_scheduled = value

    @property
    def nextflow_log(self) -> dict:
        return self.__nextflow_log

    @nextflow_log.setter
    def nextflow_log(self, value: str):
        self.__nextflow_log = value
    
    @property
    def file_names(self) -> List[pathlib.Path]:
        if self.file_directory and self.file_directory.is_dir():
            return [path.name for path in self.file_directory.iterdir() if path.is_file()]
        return []

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
            "nextflow_workflow_type": self.nextflow_workflow_type,
            "nextflow_log": self.nextflow_log,
            "is_scheduled": self.is_scheduled,
            "files": self.file_names
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
            INSERT_QUERY = f"INSERT INTO {self.__class__.TABLE_NAME} (name, nextflow_workflow, nextflow_workflow_type, nextflow_arguments, is_scheduled, nextflow_log) VALUES (%s, %s, %s, %s, %s, %s) RETURNING ID;"
            database_cursor.execute(
                INSERT_QUERY,
                (
                    self.name,
                    self.nextflow_workflow,
                    self.nextflow_workflow_type,
                    self.nextflow_arguments,
                    self.is_scheduled,
                    json.dumps(self.nextflow_log)
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
            INSERT_QUERY = f"UPDATE {self.__class__.TABLE_NAME} SET name = %s, nextflow_workflow = %s, nextflow_workflow_type = %s, nextflow_arguments = %s, is_scheduled = %s, nextflow_log = %s WHERE id = %s;"
            database_cursor.execute(
                INSERT_QUERY,
                (
                    self.name,
                    self.nextflow_workflow,
                    self.nextflow_workflow_type,
                    self.nextflow_arguments,
                    self.is_scheduled,
                    json.dumps(self.nextflow_log),
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

    def add_file(self, filename: str, file: Union[io.BytesIO, io.StringIO]):
        with self.file_directory.joinpath(filename).open("wb") as workflow_file:
            workflow_file.write(file)

    def remove_file(self, filename: str) -> bool:
        """
        Removes the given file from the file directory.

        Parameters
        ----------
        filename : str
            File name

        Returns
        -------
        Returns true (file was deleted) or false (file does not exists) 
        """
        file_path = self.file_directory.joinpath(filename)
        if file_path.is_file():
            file_path.unlink()
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
            "nextflow_workflow_type": self.nextflow_workflow_type,
            "nextflow_arguments": self.nextflow_arguments.replace("\n", " ")
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
        select_query = f"SELECT id, name, nextflow_workflow, nextflow_workflow_type, nextflow_arguments, is_scheduled, nextflow_log FROM {cls.TABLE_NAME}"
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
